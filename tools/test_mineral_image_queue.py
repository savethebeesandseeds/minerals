"""Exercise queue ownership and recovery with isolated temporary catalogs."""

from concurrent.futures import ThreadPoolExecutor
import hashlib
import importlib.util
import json
from pathlib import Path
import sqlite3
import struct
import tempfile
import threading
import time
import unittest
import zlib


spec = importlib.util.spec_from_file_location(
    'mineral_image_queue', Path(__file__).with_name('mineral-image-queue.py'))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
ImageQueue = module.ImageQueue


def png_fixture():
    def chunk(kind, payload):
        return (struct.pack('>I', len(payload)) + kind + payload +
                struct.pack('>I', zlib.crc32(kind + payload) & 0xffffffff))
    return (b'\x89PNG\r\n\x1a\n' +
            chunk(b'IHDR', struct.pack('>IIBBBBB', 1, 1, 8, 2, 0, 0, 0)) +
            chunk(b'IDAT', zlib.compress(b'\x00\x00\x00\x00')) + chunk(b'IEND', b''))


class QueueTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='minerals-image-queue-test-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.data = self.root / 'private-data'
        catalog = self.root / 'public-catalog'
        catalog.mkdir()
        source = catalog / 'catalog.sqlite3'
        self.ids = ['mat_' + f'{i:032x}' for i in range(12)]
        with sqlite3.connect(source) as db:
            db.execute('CREATE TABLE minerals(public_id TEXT,canonical_name TEXT,formula TEXT,'
                       'description TEXT,properties_json TEXT)')
            db.executemany('INSERT INTO minerals VALUES(?,?,?,?,?)',
                           [(public_id, f'Mineral {i:02}', 'SiO2', '', '{}')
                            for i, public_id in enumerate(self.ids)])
        self.manifest_path = catalog / 'catalog-manifest.json'
        self.manifest = {'release_id': 'fixture-release-1', 'mineral_count': len(self.ids),
                         'database': {'path': source.name,
                                      'sha256': 'sha256:' + hashlib.sha256(source.read_bytes()).hexdigest()}}
        self.manifest_path.write_text(json.dumps(self.manifest))
        (self.root / 'docs').mkdir()
        (self.root / 'docs/MINERAL_IMAGE_SINGLE_IMAGE_V1.md').write_text(
            '```text\nCreate {{mineral_name}} on black. Appearance: {{appearance_brief}}.\n```\n')

    def queue(self):
        queue = ImageQueue(self.root, self.data, lease_seconds=60)
        self.addCleanup(queue.close)
        return queue

    def save(self, public_id, extension='.png', content=None):
        images = self.data / 'images'
        images.mkdir(parents=True, exist_ok=True)
        path = images / (public_id + extension)
        path.write_bytes(png_fixture() if content is None else content)
        return path

    def test_concurrent_initialization_and_claims_are_unique(self):
        barrier = threading.Barrier(6)

        def claim(index):
            barrier.wait(timeout=10)
            queue = ImageQueue(self.root, self.data)
            try:
                return queue.next(f'worker-{index}')
            finally:
                queue.close()

        with ThreadPoolExecutor(max_workers=6) as pool:
            packets = list(pool.map(claim, range(6)))
        self.assertEqual(len({p['public_id'] for p in packets}), 6)
        self.assertEqual(len({p['claim_token'] for p in packets}), 6)
        self.assertTrue(all(p['generation_allowed'] for p in packets))
        self.assertEqual(self.queue().status()['counts']['in_progress'], 6)

    def test_same_worker_gets_existing_assignment_without_new_attempt(self):
        queue = self.queue()
        first = queue.next('session-a')
        again = queue.next('session-a')
        self.assertEqual(first['claim_token'], again['claim_token'])
        self.assertFalse(again['generation_allowed'])
        self.assertEqual(again['claim_count'], 1)
        self.assertIn('Mineral 00', first['prompt_template'])
        self.assertTrue(first['save_path'].endswith(self.ids[0] + '.png'))

    def test_existing_image_is_adopted_and_legacy_filename_is_ignored(self):
        self.save(self.ids[0])
        self.save('legacy-specimen', content=b'unrelated asset')
        queue = self.queue()
        self.assertEqual(queue.next('session-a')['public_id'], self.ids[1])
        self.assertEqual(queue.status()['counts']['complete'], 1)

    def test_wrong_worker_and_token_cannot_complete(self):
        queue = self.queue()
        job = queue.next('session-a')
        self.save(job['public_id'])
        with self.assertRaisesRegex(ValueError, 'wrong worker'):
            queue.complete('session-b', job['claim_token'])
        with self.assertRaisesRegex(ValueError, 'Unknown/stale token'):
            queue.complete('session-a', 'fabricated-token')
        result = queue.complete('session-a', job['claim_token'])
        self.assertEqual(result['width'], 1)
        self.assertEqual(result['sha256'], hashlib.sha256(png_fixture()).hexdigest())
        self.assertEqual(queue.complete('session-a', job['claim_token'])['result'], 'complete')

    def test_partial_copy_stays_reserved(self):
        queue = self.queue()
        job = queue.next('session-a')
        self.save(job['public_id'], content=png_fixture()[:-4])
        with self.assertRaises(ValueError):
            queue.complete('session-a', job['claim_token'])
        self.assertEqual(queue.status()['counts']['in_progress'], 1)
        self.assertEqual(queue.next('session-a')['public_id'], job['public_id'])
        self.assertNotEqual(queue.next('session-b')['public_id'], job['public_id'])

    def test_expired_reservation_is_not_automatically_reassigned(self):
        queue = self.queue()
        first = queue.next('session-a')
        queue.db.execute('UPDATE jobs SET lease_until=? WHERE public_id=?',
                         (time.time() - 1, first['public_id']))
        self.assertTrue(queue.next('session-a')['stale'])
        self.assertNotEqual(queue.next('session-b')['public_id'], first['public_id'])
        self.assertEqual(queue.status()['counts']['stale_in_progress'], 1)
        queue.requeue(first['public_id'], 'Confirmed image call never started', True)
        replacement = queue.next('session-c')
        self.assertEqual(replacement['public_id'], first['public_id'])
        self.assertNotEqual(replacement['claim_token'], first['claim_token'])
        self.assertEqual(replacement['claim_count'], 2)
        with self.assertRaisesRegex(ValueError, 'Unknown/stale token'):
            queue.complete('session-a', first['claim_token'])

    def test_uncertain_call_blocks_same_worker_until_reconciled(self):
        queue = self.queue()
        job = queue.next('session-a')
        queue.fail('session-a', job['claim_token'], 'Tool result not yet available', 'uncertain')
        self.assertEqual(queue.next('session-a')['state'], 'uncertain')
        self.assertFalse(queue.next('session-a')['generation_allowed'])
        with self.assertRaises(ValueError):
            queue.requeue(job['public_id'], 'Still uncertain', False)
        with self.assertRaises(ValueError):
            queue.requeue(job['public_id'], '', True)

    def test_confirmed_failure_or_research_hold_allows_next_record(self):
        queue = self.queue()
        for kind in ('failed', 'needs_research'):
            job = queue.next('session-a')
            queue.fail('session-a', job['claim_token'], 'Needs attention', kind)
            self.assertNotEqual(queue.next('session-a')['public_id'], job['public_id'])
        counts = queue.status()['counts']
        self.assertEqual(counts['failed'], 1)
        self.assertEqual(counts['needs_research'], 1)

    def test_saved_image_reconciles_after_session_interruption(self):
        queue = self.queue()
        job = queue.next('session-a')
        self.save(job['public_id'])
        queue2 = self.queue()
        self.assertEqual(queue2.status()['counts']['complete'], 1)
        self.assertNotEqual(queue2.next('session-a')['public_id'], job['public_id'])
        self.assertEqual(queue.complete('session-a', job['claim_token'])['result'], 'complete')

    def test_missing_or_corrupted_complete_image_needs_review(self):
        queue = self.queue()
        self.save(self.ids[0])
        second = self.save(self.ids[1])
        self.assertEqual(queue.status()['counts']['complete'], 2)
        (queue.images / (self.ids[0] + '.png')).unlink()
        second.write_bytes(b'invalid')
        counts = queue.status()['counts']
        self.assertEqual(counts['complete'], 0)
        self.assertEqual(counts['failed'], 2)
        with self.assertRaisesRegex(ValueError, 'Existing image needs review'):
            queue.requeue(self.ids[1], 'No generation active', True)

    def test_duplicate_or_symlink_images_are_not_adopted(self):
        queue = self.queue()
        target = self.save(self.ids[0])
        self.save(self.ids[0], '.PNG')
        (queue.images / (self.ids[1] + '.png')).symlink_to(target)
        counts = queue.status()['counts']
        self.assertEqual(counts['complete'], 0)
        self.assertEqual(counts['failed'], 2)

    def test_heartbeat_extends_only_owned_active_job(self):
        queue = self.queue()
        job = queue.next('session-a')
        queue.db.execute('UPDATE jobs SET lease_until=? WHERE public_id=?',
                         (time.time() - 1, job['public_id']))
        queue.heartbeat('session-a', job['claim_token'])
        self.assertFalse(queue.next('session-a')['stale'])
        with self.assertRaises(ValueError):
            queue.heartbeat('other-worker', job['claim_token'])

    def test_catalog_hash_mismatch_is_rejected(self):
        self.manifest['database']['sha256'] = 'sha256:incorrect'
        self.manifest_path.write_text(json.dumps(self.manifest))
        with self.assertRaisesRegex(ValueError, 'hash mismatch'):
            ImageQueue(self.root, self.data)

    def test_new_catalog_release_does_not_reset_assignments(self):
        queue = self.queue()
        job = queue.next('session-a')
        self.manifest['release_id'] = 'fixture-release-2'
        self.manifest_path.write_text(json.dumps(self.manifest))
        with self.assertRaisesRegex(ValueError, 'Catalog release changed'):
            ImageQueue(self.root, self.data)
        self.assertEqual(queue.next('session-a')['claim_token'], job['claim_token'])

    def test_no_pending_records_returns_counts(self):
        queue = self.queue()
        for public_id in self.ids:
            self.save(public_id)
        result = queue.next('session-a')
        self.assertEqual(result['result'], 'no_job')
        self.assertEqual(result['counts']['total'], len(self.ids))
        self.assertEqual(result['counts']['complete'], len(self.ids))


if __name__ == '__main__':
    unittest.main()
