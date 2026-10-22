"""Exercise private COD extraction coordination using isolated fixtures only.

Run inside minerals-admin. No test opens the real registry, cache or queues.
"""

from concurrent.futures import ProcessPoolExecutor
import copy
import hashlib
import importlib.util
import json
import multiprocessing
from pathlib import Path
import sqlite3
import sys
import tempfile
import time
import unittest
from unittest.mock import patch


sys.path.insert(0, str(Path(__file__).parent))
spec = importlib.util.spec_from_file_location(
    'cod_extraction_queue', Path(__file__).with_name('cod-extraction-queue.py'))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
ExtractionQueue = module.ExtractionQueue


def independent_claim(arguments):
    root, data, worker, start_at = arguments
    delay = start_at - time.time()
    if delay > 0:
        time.sleep(delay)
    queue = ExtractionQueue(root, data, lease_seconds=60)
    try:
        return queue.next(worker)
    finally:
        queue.close()


class ExtractionQueueTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='cod-extraction-queue-test-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.data = self.root / 'private-data'
        self.data.mkdir()
        self.ids = ['mat_' + f'{index:032x}' for index in range(6)]
        self.registry = self.data / 'minerals.db'
        self.properties = {}
        with sqlite3.connect(self.registry) as db:
            db.executescript('''
                CREATE TABLE materials(
                    id INTEGER PRIMARY KEY, public_id TEXT UNIQUE,
                    canonical_name TEXT, formula TEXT, description TEXT,
                    record_type TEXT, properties_json TEXT, synonyms_json TEXT,
                    identifiers_json TEXT, nomenclature_status TEXT,
                    is_valid_species INTEGER, verification_status TEXT,
                    search_text TEXT, updated_at TEXT);
                CREATE TABLE material_evidence(
                    id INTEGER PRIMARY KEY, material_id INTEGER,
                    source_release_id TEXT, claim_json TEXT,
                    review_status TEXT, claim_scope TEXT, source_id INTEGER);
            ''')
            for index, public_id in enumerate(self.ids):
                properties = {
                    'cod_records': [{'cod_id': str(1000000 + index), 'revision': '1234',
                                     'status': 'name_matched',
                                     'relationship': 'exact_name_candidate',
                                     'primary_assignment': False}],
                    'publication_observations': [{'claim_id': 'existing-' + public_id,
                                                   'value': 'Prior reviewed evidence'}],
                    'unrelated_nested_data': {'retain': [1, 2, {'a': 'b'}]},
                }
                self.properties[public_id] = properties
                db.execute('INSERT INTO materials VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)', (
                    index + 1, public_id, f'Mineral {index:02}', 'SiO2', 'Prior description',
                    'mineral', json.dumps(properties), '[]', '{}', 'approved', 1,
                    'reviewed', '', 'fixture'))
        catalog = self.root / 'public-catalog'
        catalog.mkdir()
        self.catalog = catalog / 'catalog.sqlite3'
        with sqlite3.connect(self.catalog) as db:
            db.execute('CREATE TABLE minerals(public_id TEXT,canonical_name TEXT,'
                       'formula TEXT,description TEXT,properties_json TEXT)')
            db.executemany('INSERT INTO minerals VALUES(?,?,?,?,?)', [
                (public_id, f'Mineral {index:02}', 'SiO2', 'Prior description', '{}')
                for index, public_id in enumerate(self.ids)])
        self.write_json(catalog / 'catalog-manifest.json', {
            'release_id': 'fixture-release-1', 'mineral_count': len(self.ids),
            'database': {'path': self.catalog.name, 'bytes': self.catalog.stat().st_size,
                         'sha256': 'sha256:' + self.digest(self.catalog)}})
        # A separate queue must never be opened, replaced or used for these assignments.
        self.content_queue = self.data / 'pilots/mineral-content-queue-v1/queue.sqlite3'
        self.content_queue.parent.mkdir(parents=True)
        self.content_queue.write_bytes(b'untouched separate content queue fixture')
        self.original_registry = self.registry.read_bytes()
        self.original_catalog = self.catalog.read_bytes()
        self.original_content_queue = self.content_queue.read_bytes()
        self.sources = {
            (str(1000000 + index), '1234'): self.source(str(1000000 + index), '1234')
            for index in range(len(self.ids))}

    @staticmethod
    def digest(path):
        return hashlib.sha256(path.read_bytes()).hexdigest()

    @staticmethod
    def write_json(path, value):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n')
        return path

    @staticmethod
    def source(cod_id, revision):
        return {
            'format': 'waajacu-cod-metadata-candidate-v1', 'schema_version': 1,
            'cod_id': cod_id, 'revision': revision,
            'review_status': 'unreviewed', 'needs_review': True,
            'data_basis': 'saved_cod_metadata',
            'metadata_locator': {
                'raw_body': {'path': 'objects/sha256/' + 'a' * 64,
                             'sha256': 'sha256:' + 'a' * 64, 'bytes': 12},
                'json_pointer': '/0', 'request_ordinal': 0,
                'retrieved_at': '2026-10-01T00:00:00Z',
                'response_url': 'https://www.crystallography.net/cod/result',
                'snapshot_sha256': 'sha256:' + 'b' * 64},
            'raw_row': {'file': cod_id, 'svnrevision': revision,
                        'a': '5.10(2)', 'compoundsource': 'Synthetic'},
            'source': {'record_url': f'https://www.crystallography.net/cod/{cod_id}.html',
                       'pinned_cif_url': f'https://www.crystallography.net/cod/{cod_id}.cif@{revision}',
                       'bibliography': {'title': 'A source-specific model'}},
            'mapped': {'unit_cell': {}, 'space_group': {}, 'formulas': {},
                       'formula_units_Z': {}, 'volume': {}, 'conditions': {}, 'method': {}},
            'origin': {'raw_value': 'Synthetic', 'markers': ['synthetic']},
            'review_flags': ['synthetic_origin'],
        }

    def load_sources(self, data, wanted):
        self.assertEqual(Path(data), self.data)
        return {pair: copy.deepcopy(self.sources[pair]) for pair in wanted}

    def queue(self, prepare=True):
        queue = ExtractionQueue(self.root, self.data, lease_seconds=60)
        self.addCleanup(queue.close)
        if prepare:
            queue.prepare(source_loader=self.load_sources)
        return queue

    def review(self, job):
        return {
            'material_public_id': job['public_id'],
            'claim_token': job['claim_token'], 'packet_sha256': job['packet_sha256'],
            'review_note': 'Checked saved source rows; synthetic origin remains qualified.',
            'source_checks': [
                {'cod_id': source['link']['cod_id'], 'revision': source['link']['revision'],
                 'disposition': 'checked',
                 'note': 'Source cell fields and synthetic marker checked against frozen row.'}
                for source in job['sources']],
            'remaining_gaps': ['Natural specimen relationship requires separate scientific review.'],
        }

    def review_file(self, job, value=None):
        return self.write_json(Path(job['review_file']), self.review(job) if value is None else value)

    def assert_sources_unchanged(self):
        self.assertEqual(self.registry.read_bytes(), self.original_registry)
        self.assertEqual(self.catalog.read_bytes(), self.original_catalog)
        self.assertEqual(self.content_queue.read_bytes(), self.original_content_queue)

    def test_prepare_is_explicit_and_repeated_prepare_preserves_claim(self):
        queue = self.queue(prepare=False)
        with self.assertRaises(ValueError):
            queue.next('session-a')
        queue.prepare(source_loader=self.load_sources)
        first = queue.next('session-a')
        queue.prepare(source_loader=self.load_sources)
        second = queue.next('session-a')
        self.assertEqual(first['public_id'], second['public_id'])
        self.assertEqual(first['claim_token'], second['claim_token'])
        self.assert_sources_unchanged()

    def test_completed_queue_remains_readable_after_approved_public_release(self):
        queue = self.queue()
        for _ in self.ids:
            job = queue.next('session-a')
            queue.complete('session-a', job['claim_token'], self.review_file(job), confirm_reviewed=True)
        manifest_path = self.root / 'public-catalog/catalog-manifest.json'
        manifest = json.loads(manifest_path.read_bytes())
        manifest['release_id'] = 'approved-follow-up-release'
        self.write_json(manifest_path, manifest)
        self.assertEqual(queue.status()['counts']['staged'], len(self.ids))
        self.assertEqual(queue.next('session-a')['result'], 'no_job')
        self.assertEqual(queue.ready()['catalog_release_id'], 'fixture-release-1')

    def test_independent_processes_claim_distinct_records_and_tokens(self):
        self.queue()
        start_at = time.time() + 0.5
        arguments = [(str(self.root), str(self.data), f'worker-{index}', start_at)
                     for index in range(4)]
        with ProcessPoolExecutor(max_workers=4,
                                 mp_context=multiprocessing.get_context('fork')) as pool:
            jobs = list(pool.map(independent_claim, arguments))
        self.assertEqual(len({job['public_id'] for job in jobs}), 4)
        self.assertEqual(len({job['claim_token'] for job in jobs}), 4)
        self.assert_sources_unchanged()

    def test_same_worker_concurrently_resumes_one_immutable_packet(self):
        self.queue()
        start_at = time.time() + 0.5
        arguments = [(str(self.root), str(self.data), 'one-session', start_at)] * 4
        with ProcessPoolExecutor(max_workers=4,
                                 mp_context=multiprocessing.get_context('fork')) as pool:
            jobs = list(pool.map(independent_claim, arguments))
        self.assertEqual(len({job['public_id'] for job in jobs}), 1)
        self.assertEqual(len({job['claim_token'] for job in jobs}), 1)
        self.assertEqual(len({job['packet_file'] for job in jobs}), 1)
        self.assertEqual(len({job['packet_sha256'] for job in jobs}), 1)

    def test_resume_expands_same_assignment_and_preserves_original_context(self):
        queue = self.queue()
        compact = queue.next('session-a', compact=True)
        expanded = queue.next('session-a', compact=False)
        self.assertNotIn('sources', compact)
        self.assertNotIn('record_context', compact)
        self.assertEqual(compact['source_count'], len(expanded['sources']))
        self.assertEqual(compact['claim_token'], expanded['claim_token'])
        self.assertEqual(compact['packet_sha256'], expanded['packet_sha256'])
        packet = json.loads(Path(expanded['packet_file']).read_bytes())
        self.assertEqual(packet['sources'][0]['candidate']['raw_row']['a'], '5.10(2)')
        self.assertEqual(packet['sources'][0]['candidate']['origin']['markers'], ['synthetic'])
        self.assertEqual(packet['sources'][0]['candidate']['review_status'], 'unreviewed')
        self.assert_sources_unchanged()

    def test_completion_stages_private_artifact_without_scientific_promotion(self):
        queue = self.queue()
        job = queue.next('session-a')
        packet_bytes = Path(job['packet_file']).read_bytes()
        result = queue.complete('session-a', job['claim_token'], self.review_file(job),
                                confirm_reviewed=True)
        self.assertEqual(result['result'], 'staged')
        self.assertFalse(result['scientific_acceptance'])
        self.assertEqual(Path(job['packet_file']).read_bytes(), packet_bytes)
        self.assert_sources_unchanged()
        next_job = queue.next('session-a')
        self.assertNotEqual(next_job['public_id'], job['public_id'])
        packet = json.loads(packet_bytes)
        self.assertEqual(packet['sources'][0]['candidate']['review_status'], 'unreviewed')
        self.assertTrue(packet['sources'][0]['candidate']['needs_review'])

    def test_completion_requires_explicit_review_confirmation(self):
        queue = self.queue()
        job = queue.next('session-a')
        with self.assertRaises(ValueError):
            queue.complete('session-a', job['claim_token'], self.review_file(job))
        self.assertEqual(queue.next('session-a')['claim_token'], job['claim_token'])

    def test_completion_binds_worker_token_record_and_packet_hash(self):
        queue = self.queue()
        job = queue.next('session-a')
        variants = []
        for field, value in [('material_public_id', self.ids[-1]),
                             ('claim_token', 'stale-token'), ('packet_sha256', '0' * 64)]:
            review = self.review(job)
            review[field] = value
            variants.append((field, 'session-a', job['claim_token'], review))
        variants.append(('foreign-worker', 'session-b', job['claim_token'], self.review(job)))
        variants.append(('foreign-token', 'session-a', 'stale-token', self.review(job)))
        for label, worker, token, review in variants:
            with self.subTest(label=label):
                with self.assertRaises(ValueError):
                    queue.complete(worker, token, self.review_file(job, review), confirm_reviewed=True)
                self.assertEqual(queue.next('session-a')['claim_token'], job['claim_token'])

    def test_each_source_requires_one_explicit_check_or_deferral(self):
        queue = self.queue()
        job = queue.next('session-a')
        base = self.review(job)
        invalid = []
        changed = copy.deepcopy(base)
        changed['source_checks'] = []
        invalid.append(('missing', changed))
        changed = copy.deepcopy(base)
        changed['source_checks'] *= 2
        invalid.append(('duplicate', changed))
        for field, value in [('cod_id', '9999999'), ('revision', 'unfrozen-revision'),
                             ('disposition', 'accepted'), ('note', '')]:
            changed = copy.deepcopy(base)
            changed['source_checks'][0][field] = value
            invalid.append((field, changed))
        for label, review in invalid:
            with self.subTest(label=label):
                with self.assertRaises(ValueError):
                    queue.complete('session-a', job['claim_token'], self.review_file(job, review),
                                   confirm_reviewed=True)
        base['source_checks'][0]['disposition'] = 'deferred'
        base['source_checks'][0]['note'] = 'The frozen row lacks sufficient specimen provenance.'
        result = queue.complete('session-a', job['claim_token'], self.review_file(job, base),
                                confirm_reviewed=True)
        self.assertEqual(result['result'], 'staged')

    def test_tampered_packet_cannot_complete(self):
        queue = self.queue()
        job = queue.next('session-a')
        self.review_file(job)
        packet = json.loads(Path(job['packet_file']).read_bytes())
        packet['sources'][0]['candidate']['raw_row']['a'] = '999'
        self.write_json(Path(job['packet_file']), packet)
        with self.assertRaises(ValueError):
            queue.complete('session-a', job['claim_token'], Path(job['review_file']),
                           confirm_reviewed=True)
        self.assert_sources_unchanged()

    def test_review_artifacts_must_stay_private_and_not_be_symlinks(self):
        queue = self.queue()
        job = queue.next('session-a')
        outside = self.write_json(self.root / 'outside.json', self.review(job))
        original = self.review_file(job)
        linked = self.data / 'linked-review.json'
        linked.symlink_to(original)
        for path in (outside, linked):
            with self.subTest(path=str(path)):
                with self.assertRaises(ValueError):
                    queue.complete('session-a', job['claim_token'], path, confirm_reviewed=True)

    def test_failure_preserves_reservation_until_explicit_requeue(self):
        queue = self.queue()
        job = queue.next('session-a')
        queue.fail('session-a', job['claim_token'], 'Frozen source is incomplete; retain for reconciliation.')
        with self.assertRaises(ValueError):
            queue.requeue(job['public_id'], 'Retry without confirmation', confirmed_idle=False)
        another = queue.next('session-b')
        self.assertNotEqual(another['public_id'], job['public_id'])
        old_packet = Path(job['packet_file']).read_bytes()
        queue.requeue(job['public_id'], 'Reviewer is idle; source issue reconciled.', confirmed_idle=True)
        self.assertEqual(Path(job['packet_file']).read_bytes(), old_packet)
        self.assert_sources_unchanged()

    def test_requeue_rejects_staged_job_without_erasing_immutable_history(self):
        queue = self.queue()
        job = queue.next('session-a')
        queue.complete('session-a', job['claim_token'], self.review_file(job), confirm_reviewed=True)
        packet_path = Path(job['packet_file'])
        old_packet = packet_path.read_bytes()
        files = {path: path.read_bytes() for path in packet_path.parent.parent.rglob('*.json')}
        with self.assertRaises(ValueError):
            queue.requeue(job['public_id'], 'New independent review requested.', confirmed_idle=True)
        self.assertEqual(packet_path.read_bytes(), old_packet)
        for path, value in files.items():
            with self.subTest(path=str(path)):
                self.assertTrue(path.is_file())
                self.assertEqual(path.read_bytes(), value)
        self.assert_sources_unchanged()

    def test_expired_lease_stays_reserved_and_can_be_renewed_by_owner(self):
        queue = self.queue()
        job = queue.next('session-a')
        queue.db.execute('UPDATE jobs SET lease_until=? WHERE public_id=?',
                         (time.time() - 30, job['public_id']))
        held = queue.next('session-a')
        self.assertTrue(held['stale'])
        self.assertEqual(held['claim_token'], job['claim_token'])
        another = queue.next('session-b')
        self.assertNotEqual(another['public_id'], job['public_id'])
        with self.assertRaises(ValueError):
            queue.heartbeat('session-b', job['claim_token'])
        queue.heartbeat('session-a', job['claim_token'])
        renewed = queue.next('session-a')
        self.assertFalse(renewed['stale'])
        self.assertEqual(renewed['packet_sha256'], job['packet_sha256'])

    def test_requeue_changes_token_but_retains_old_packet(self):
        queue = self.queue()
        job = queue.next('session-a')
        original = Path(job['packet_file']).read_bytes()
        queue.fail('session-a', job['claim_token'], 'Session stopped before finishing checks.')
        queue.requeue(job['public_id'], 'Session confirmed idle.', confirmed_idle=True)
        replacement = queue.next('session-b')
        self.assertEqual(replacement['public_id'], job['public_id'])
        self.assertNotEqual(replacement['claim_token'], job['claim_token'])
        self.assertEqual(Path(job['packet_file']).read_bytes(), original)
        with self.assertRaises(ValueError):
            queue.complete('session-a', job['claim_token'], self.review_file(job), confirm_reviewed=True)

    def test_existing_models_remain_in_packet_and_are_still_checked(self):
        properties = self.properties[self.ids[0]]
        existing = {'cod_id': '1000000', 'revision': '1234',
                    'sample_origin': 'synthetic', 'temperature': '80 K',
                    'cell': {'a': {'value': 5.05, 'standard_uncertainty': 0.02}},
                    'review_status': 'reviewed', 'qualification': 'Prior source-specific determination'}
        properties['cod_structure_observations'] = [existing]
        properties['cod_records'].append({'cod_id': '1000010', 'revision': '1235',
                                          'status': 'name_matched',
                                          'relationship': 'exact_name_candidate',
                                          'primary_assignment': False})
        self.sources[('1000010', '1235')] = self.source('1000010', '1235')
        with sqlite3.connect(self.registry) as db:
            db.execute('UPDATE materials SET properties_json=? WHERE public_id=?',
                       (json.dumps(properties), self.ids[0]))
        self.original_registry = self.registry.read_bytes()
        queue = self.queue()
        job = queue.next('session-a')
        self.assertEqual(len(job['sources']), 2)
        self.assertEqual(job['sources'][0]['existing_models'], [existing])
        self.assertEqual(job['sources'][0]['action'], 'inspect_existing_and_fill_gaps')
        self.assertEqual(job['sources'][1]['action'], 'check_metadata_extraction')
        review = self.review(job)
        review['source_checks'] = review['source_checks'][1:]
        with self.assertRaises(ValueError):
            queue.complete('session-a', job['claim_token'], self.review_file(job, review),
                           confirm_reviewed=True)
        queue.complete('session-a', job['claim_token'], self.review_file(job), confirm_reviewed=True)
        self.assert_sources_unchanged()

    def test_duplicate_pair_associations_preserve_every_status_without_selecting_one(self):
        properties = copy.deepcopy(self.properties[self.ids[0]])
        original = copy.deepcopy(properties['cod_records'][0])
        reviewed = dict(original, status='identified', primary_assignment=True,
                        relationship='publication_supported',
                        qualification='Source-supported primary assignment from an earlier review')
        properties['cod_records'].append(reviewed)
        with sqlite3.connect(self.registry) as db:
            db.execute('UPDATE materials SET properties_json=? WHERE public_id=?',
                       (json.dumps(properties), self.ids[0]))
        self.original_registry = self.registry.read_bytes()
        queue = self.queue()
        job = queue.next('session-a')
        self.assertEqual(len(job['sources']), 1)
        item = job['sources'][0]
        self.assertEqual(item['associations'], [original, reviewed])
        self.assertEqual(item['link'], {'cod_id': '1000000', 'revision': '1234'})
        self.assertNotIn('status', item['link'])
        self.assertNotIn('relationship', item['link'])
        self.assertNotIn('primary_assignment', item['link'])
        self.assertIn('multiple_saved_associations', item['association_review_flags'])
        self.assertIn('differing_link_metadata', item['association_review_flags'])
        retained = job['record_context']['material']['properties_json']['cod_records']
        self.assertEqual(retained, [original, reviewed])
        review = self.review(job)
        self.assertEqual(len(review['source_checks']), 1)
        result = queue.complete('session-a', job['claim_token'], self.review_file(job, review),
                                confirm_reviewed=True)
        self.assertEqual(result['result'], 'staged')
        self.assert_sources_unchanged()

    def test_catalog_release_change_requires_reconciliation_without_resetting_claim(self):
        queue = self.queue()
        job = queue.next('session-a')
        manifest_path = self.root / 'public-catalog/catalog-manifest.json'
        manifest = json.loads(manifest_path.read_bytes())
        manifest['release_id'] = 'different-release'
        self.write_json(manifest_path, manifest)
        with self.assertRaises(ValueError):
            queue.next('session-b')
        manifest['release_id'] = 'fixture-release-1'
        self.write_json(manifest_path, manifest)
        self.assertEqual(queue.next('session-a')['claim_token'], job['claim_token'])

    def test_prepared_snapshot_is_not_replaced_by_later_live_edits_or_mapper_changes(self):
        queue = self.queue()
        properties = copy.deepcopy(self.properties[self.ids[0]])
        properties['cod_records'][0]['relationship'] = 'later_relationship_edit'
        properties['publication_observations'][0]['value'] = 'Later independent research'
        with sqlite3.connect(self.registry) as db:
            db.execute('UPDATE materials SET properties_json=? WHERE public_id=?',
                       (json.dumps(properties), self.ids[0]))
        self.original_registry = self.registry.read_bytes()
        self.sources[('1000000', '1234')]['raw_row']['a'] = 'changed mapper input'

        def must_not_reload(data, wanted):
            raise AssertionError('Prepared snapshot must not silently rerun the mapper')

        queue.prepare(source_loader=must_not_reload)
        job = queue.next('session-a')
        self.assertEqual(job['sources'][0]['link']['relationship'], 'exact_name_candidate')
        self.assertEqual(job['sources'][0]['candidate']['raw_row']['a'], '5.10(2)')
        prior = job['record_context']['material']['properties_json']['publication_observations']
        self.assertEqual(prior[0]['value'], 'Prior reviewed evidence')
        self.assert_sources_unchanged()

    def test_retry_after_receipt_write_interruption_reuses_durable_artifacts(self):
        queue = self.queue()
        job = queue.next('session-a')
        review = self.review_file(job)
        publish = module.immutable_file

        def interrupted_after_receipt(path, raw):
            publish(path, raw)
            if path.name == 'receipt.json':
                raise OSError('Simulated interruption after durable receipt, before SQLite commit')

        with patch.object(module, 'immutable_file', interrupted_after_receipt):
            with self.assertRaises(OSError):
                queue.complete('session-a', job['claim_token'], review, confirm_reviewed=True)
        before_retry = {path: path.read_bytes() for path in queue.work.rglob('*.json')
                        if 'drafts' not in path.parts}
        self.assertTrue(any(path.name == 'receipt.json' for path in before_retry))
        self.assertEqual(queue.next('session-a')['claim_token'], job['claim_token'])
        result = queue.complete('session-a', job['claim_token'], review, confirm_reviewed=True)
        self.assertEqual(result['result'], 'staged')
        for path, value in before_retry.items():
            with self.subTest(path=str(path)):
                self.assertEqual(path.read_bytes(), value)
        self.assert_sources_unchanged()

    def test_prepare_failure_does_not_partially_seed_assignments(self):
        queue = self.queue(prepare=False)

        def missing_sources(data, wanted):
            result = self.load_sources(data, wanted)
            result.pop(next(iter(result)))
            return result

        with self.assertRaises(ValueError):
            queue.prepare(source_loader=missing_sources)
        queue.prepare(source_loader=self.load_sources)
        jobs = [queue.next(f'worker-{index}') for index in range(len(self.ids))]
        self.assertEqual({job['public_id'] for job in jobs}, set(self.ids))
        self.assert_sources_unchanged()

    def test_failed_candidate_schema_cannot_be_prepared_as_reviewed(self):
        queue = self.queue(prepare=False)
        self.sources[('1000000', '1234')]['review_status'] = 'reviewed'
        self.sources[('1000000', '1234')]['needs_review'] = False
        with self.assertRaises(ValueError):
            queue.prepare(source_loader=self.load_sources)
        self.assert_sources_unchanged()


if __name__ == '__main__':
    unittest.main()
