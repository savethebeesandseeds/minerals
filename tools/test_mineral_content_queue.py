"""Exercise research reservations and save verification using isolated fixtures.

Run in minerals-admin. These tests never open the real mineral database, queue,
review history or public catalogue.
"""

from concurrent.futures import ProcessPoolExecutor
from contextlib import redirect_stdout
import copy
import gzip
import hashlib
import importlib.util
import io
import json
import multiprocessing
from pathlib import Path
import sqlite3
import sys
import tempfile
import time
import unittest
from unittest.mock import patch


spec = importlib.util.spec_from_file_location(
    'mineral_content_queue', Path(__file__).with_name('mineral-content-queue.py'))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
ContentQueue = module.ContentQueue


def independent_claim(arguments):
    """Every process opens its own SQLite connection and initializes the queue."""
    root, data, worker, start_at = arguments
    delay = start_at - time.time()
    if delay > 0:
        time.sleep(delay)
    queue = ContentQueue(root, data, lease_seconds=60)
    try:
        return queue.next(worker)
    finally:
        queue.close()


class ContentQueueTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='minerals-content-queue-test-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.data = self.root / 'private-data'
        self.data.mkdir()
        catalog = self.root / 'public-catalog'
        catalog.mkdir()
        self.ids = ['mat_' + f'{index:032x}' for index in range(12)]
        self.source = catalog / 'catalog.sqlite3'
        with sqlite3.connect(self.source) as db:
            db.execute('CREATE TABLE minerals(public_id TEXT,canonical_name TEXT,'
                       'formula TEXT,description TEXT,properties_json TEXT)')
            db.executemany('INSERT INTO minerals VALUES(?,?,?,?,?)', [
                (public_id, f'Mineral {index:02}', 'SiO2', 'Existing profile', '{}')
                for index, public_id in enumerate(self.ids)])
        self.manifest_path = catalog / 'catalog-manifest.json'
        self.manifest = {
            'release_id': 'fixture-release-1', 'mineral_count': len(self.ids),
            'database': {'path': self.source.name, 'sha256':
                         'sha256:' + hashlib.sha256(self.source.read_bytes()).hexdigest()}}
        self.write_json(self.manifest_path, self.manifest)
        self.registry = self.data / 'minerals.db'
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
                CREATE TABLE ingestion_runs(
                    id INTEGER PRIMARY KEY, source_label TEXT,
                    status TEXT, metadata_json TEXT);
            ''')
            db.executemany('INSERT INTO materials VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)', [
                (index + 1, public_id, f'Mineral {index:02}', 'SiO2', 'Existing profile',
                 'mineral', '{}', '[]', '{}', 'approved', 1, 'reviewed', '', 'fixture')
                for index, public_id in enumerate(self.ids)])
        self.write_json(self.root / 'schemas/mineral-record-questionnaire-v1.json', {
            'modules': {'appearance': {'questions': {
                'appearance.color': {'lifecycle': 'active'}}}}})

    @staticmethod
    def write_json(path, value):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n')
        return path

    def queue(self):
        queue = ContentQueue(self.root, self.data, lease_seconds=60)
        self.addCleanup(queue.close)
        return queue

    def test_priority_progress_is_separate_from_full_catalogue(self):
        self.write_json(self.data / module.REVIEW_ROOT /
                        'resumed-2026-10-08/broader-record-review-queue.json',
                        {'records': [{'material_public_id': identifier, 'state': 'not_started'}
                                     for identifier in self.ids[:2]]})
        queue = self.queue()
        queue.next('priority-reviewer')
        status = queue.status()
        priority = status['priority_groups']['broader_sparse']
        later = status['priority_groups']['catalog']
        self.assertEqual((priority['total'], priority['pending'], priority['in_progress']), (2, 1, 1))
        self.assertEqual((later['total'], later['pending']), (10, 10))
        self.assertEqual((status['counts']['total'], status['counts']['pending']), (12, 11))

    def state(self, queue, public_id):
        return dict(queue.db.execute(
            'SELECT * FROM jobs WHERE public_id=?', (public_id,)).fetchone())

    def test_priority_filter_never_claims_later_catalogue_or_changes_held_group(self):
        self.write_json(self.data / module.REVIEW_ROOT /
                        'resumed-2026-10-08/broader-record-review-queue.json',
                        {'records': [{'material_public_id': self.ids[0], 'state': 'not_started'}]})
        queue = self.queue()
        first = queue.next('sparse-session', compact=True, priority_group='broader_sparse')
        self.assertEqual(first['priority_group'], 'broader_sparse')
        self.assertEqual(queue.next('another-sparse', priority_group='broader_sparse')['result'], 'no_job')
        self.assertEqual(queue.next('sparse-session', priority_group='broader_sparse')['claim_token'],
                         first['claim_token'])
        later = queue.next('later-session', priority_group='catalog')
        self.assertEqual(later['priority_group'], 'catalog')
        with self.assertRaisesRegex(ValueError, 'different priority group'):
            queue.next('later-session', priority_group='broader_sparse')
        self.assertEqual(queue.counts()['in_progress'], 2)

    def review(self, public_id, name=None):
        index = self.ids.index(public_id)
        return {
            'material_public_id': public_id,
            'canonical_name': name or f'Mineral {index:02}',
            'review_note': 'Original publication read; specimen-specific color retained.',
            'sources': [{'url': 'https://example.org/original-paper',
                         'title': 'Original mineral description', 'publisher': 'Fixture journal',
                         'access': 'Original paper, page 4, manually read',
                         'license_spdx': 'NOASSERTION'}],
            'claims': [{'claim_id': public_id + '-color-1',
                        'question_key': 'appearance.color', 'value': 'green',
                        'source_url': 'https://example.org/original-paper',
                        'locator': 'p.4, specimen description',
                        'qualification': 'Color of the described specimen.'}],
            'editorial_draft': {'text': 'The described specimen is green.'},
            'remaining_gaps': ['Hardness was not reported in the inspected source.'],
            'deferred_claims': [], 'remaining_candidates': [],
        }

    def review_file(self, packet, value=None):
        return self.write_json(self.data / 'work' / (packet['public_id'] + '.json'),
                               self.review(packet['public_id']) if value is None else value)

    def compressed_catalog(self):
        raw = self.source.read_bytes()
        compressed = self.source.with_suffix(self.source.suffix + '.gz')
        compressed.write_bytes(gzip.compress(raw, mtime=0))
        self.manifest['database']['bytes'] = len(raw)
        self.write_json(self.manifest_path, self.manifest)
        self.source.unlink()
        return compressed

    def saved_review(self, packet, record=None, batch='fixture-review-batch'):
        """Model an existing coordinator save; queue itself must never do this."""
        record = copy.deepcopy(self.review(packet['public_id']) if record is None else record)
        checkpoint = self.data / 'work' / batch
        original_path = self.data / 'work' / (packet['public_id'] + '.json')
        record['content_queue_claim'] = {
            'claim_token': packet['claim_token'],
            'submission_sha256': hashlib.sha256(original_path.read_bytes()).hexdigest()}
        payload = {'batch_id': batch, 'coordinator_verified': {'manual_review': True},
                   'records': [record]}
        payload_path = self.write_json(checkpoint / 'accepted-reviews.json', payload)
        payload_hash = hashlib.sha256(payload_path.read_bytes()).hexdigest()
        properties = {
            'publication_observations': [dict(claim, review_batch=batch,
                                             review_status='reviewed')
                                         for claim in record['claims']],
            'publication_review_history': [{
                'review_batch': batch, 'review_note': record['review_note'],
                'sources': record['sources'], 'remaining_gaps': record['remaining_gaps'],
                'deferred_claims': record.get('deferred_claims', []),
                'remaining_candidates': record.get('remaining_candidates', []),
                'publication_status': 'private', 'public_ingestion_allowed': False,
                'coordinator_verification': payload['coordinator_verified']}],
            'research_draft_descriptions': [dict(record['editorial_draft'],
                                                review_batch=batch, private_research=True,
                                                publication_status='private', review_status='draft')],
        }
        with sqlite3.connect(self.registry) as db:
            material_id = db.execute('SELECT id FROM materials WHERE public_id=?',
                                     (packet['public_id'],)).fetchone()[0]
            db.execute('UPDATE materials SET properties_json=? WHERE id=?',
                       (json.dumps(properties), material_id))
            db.execute('INSERT INTO material_evidence VALUES(?,?,?,?,?,?,?)', (
                1, material_id, batch,
                json.dumps({'review_batch': batch,
                            'observation_ids': [c['claim_id'] for c in record['claims']]}),
                'reviewed', 'appearance.color', 1))
            db.execute('INSERT INTO ingestion_runs VALUES(?,?,?,?)', (
                1, batch, 'completed', json.dumps({
                    'payload_sha256': payload_hash, 'coordinator_verified': True,
                    'public_ingestion_allowed': False})))
        receipt = {
            'status': 'completed', 'ingestion_run_id': 1,
            'committed_at': '2026-10-17T00:00:00Z',
            'completed_material_ids': [packet['public_id']],
            'results': [{'material_public_id': packet['public_id'],
                         'canonical_name': record['canonical_name'],
                         'observations': len(record['claims'])}],
            'payload_sha256': payload_hash, 'integrity': 'ok',
            'foreign_key_violations': 0, 'FTS_integrity': 'ok',
            'conflicting_primary_targets': 0, 'public_research_and_catalog_unchanged': True,
        }
        receipt_path = self.write_json(checkpoint / 'commit-receipt.json', receipt)
        return receipt_path, receipt, payload_path, payload

    def test_independent_processes_claim_distinct_records_and_tokens(self):
        start_at = time.time() + 0.8
        arguments = [(str(self.root), str(self.data), f'worker-{index}', start_at)
                     for index in range(6)]
        with ProcessPoolExecutor(max_workers=6,
                                 mp_context=multiprocessing.get_context('fork')) as pool:
            packets = list(pool.map(independent_claim, arguments))
        self.assertEqual(len({p['public_id'] for p in packets}), 6)
        self.assertEqual(len({p['claim_token'] for p in packets}), 6)
        self.assertEqual(self.queue().status()['counts']['in_progress'], 6)

    def test_same_worker_concurrently_resumes_one_assignment(self):
        start_at = time.time() + 0.8
        arguments = [(str(self.root), str(self.data), 'one-session', start_at)] * 4
        with ProcessPoolExecutor(max_workers=4,
                                 mp_context=multiprocessing.get_context('fork')) as pool:
            packets = list(pool.map(independent_claim, arguments))
        self.assertEqual(len({p['public_id'] for p in packets}), 1)
        self.assertEqual(len({p['claim_token'] for p in packets}), 1)
        self.assertEqual(self.state(self.queue(), packets[0]['public_id'])['claim_count'], 1)

    def test_resume_survives_reopening_queue_and_keeps_record_context(self):
        first = self.queue().next('session-a')
        resumed = self.queue().next('session-a')
        self.assertEqual(first['public_id'], resumed['public_id'])
        self.assertEqual(first['claim_token'], resumed['claim_token'])
        self.assertEqual(first['canonical_name'], 'Mineral 00')
        self.assertEqual(self.state(self.queue(), first['public_id'])['claim_count'], 1)

    def test_compact_evidence_preserves_live_record_and_expands_same_assignment(self):
        queue = self.queue()
        properties = {'publication_observations': [self.review(self.ids[0])['claims'][0]],
                      'publication_review_history': [{'sources': [{'url': 'https://example.org/history'}]}],
                      'research_draft_descriptions': [{'text': 'An existing private draft.'}],
                      'unrelated_property': {'retained': True}}
        evidence = [
            (41, 1, 'cod-release', json.dumps({
                'value': {'cod_id': '1234567', 'record_url': 'https://www.crystallography.net/cod/1234567.html',
                          'pinned_structure_url': 'https://www.crystallography.net/cod/1234567.cif@1',
                          'supporting_references': [{'url': 'https://doi.org/10.1234/original'}],
                          'source_snapshot': {'abstract': 'long preserved source text ' * 12000}},
                'source_locator': 'COD1234567; saved revision1'}),
             'reviewed', 'properties.cod_candidate_review', 9),
            (42, 1, 'publication-batch', json.dumps({
                'observation_ids': [properties['publication_observations'][0]['claim_id']],
                'sources': [{'url': 'https://example.org/original-paper'}]}),
             'reviewed', 'appearance.color', 10),
        ]
        with sqlite3.connect(self.registry) as db:
            db.execute('UPDATE materials SET properties_json=? WHERE id=1', (json.dumps(properties),))
            db.executemany('INSERT INTO material_evidence VALUES(?,?,?,?,?,?,?)', evidence)
        private_before, public_before = self.registry.read_bytes(), self.source.read_bytes()
        first = queue.next('compact-session', compact=True)
        compact = first['record_context']
        self.assertTrue(first['new_assignment'])
        self.assertEqual(compact['existing_evidence_count'], 2)
        self.assertIs(compact['existing_evidence_is_summary'], True)
        self.assertEqual(compact['material']['properties_json'], properties)
        self.assertNotIn('claim_json', compact['existing_evidence'][0])
        self.assertEqual(compact['existing_evidence'][0], {
            'id': 41, 'material_id': 1, 'source_id': 9, 'source_release_id': 'cod-release',
            'claim_scope': 'properties.cod_candidate_review', 'review_status': 'reviewed',
            'source_urls': ['https://doi.org/10.1234/original',
                            'https://www.crystallography.net/cod/1234567.cif@1',
                            'https://www.crystallography.net/cod/1234567.html']})
        self.assertEqual(compact['existing_evidence'][1]['source_urls'], ['https://example.org/original-paper'])
        self.assertIn('without --compact', first['instructions'])
        self.assertIn('same assignment', first['instructions'])
        full = self.queue().next('compact-session')
        self.assertFalse(full['new_assignment'])
        for key in ('public_id', 'claim_token', 'claim_count', 'worker', 'review_file', 'lease_until_utc'):
            self.assertEqual(first[key], full[key])
        self.assertEqual(full['record_context']['material'], compact['material'])
        self.assertEqual(set(full['record_context']), {'material', 'existing_evidence'})
        self.assertEqual([item['claim_json'] for item in full['record_context']['existing_evidence']],
                         [item[3] for item in evidence])
        self.assertLess(len(json.dumps(first)), len(json.dumps(full)) * 0.1)
        self.assertEqual(self.state(queue, first['public_id'])['claim_count'], 1)
        self.assertEqual(queue.db.execute("SELECT count(*) FROM events WHERE action='claimed'").fetchone()[0], 1)
        self.assertEqual(self.registry.read_bytes(), private_before)
        self.assertEqual(self.source.read_bytes(), public_before)

    def test_compact_legacy_evidence_keeps_source_metadata_and_flags_invalid_claim_json(self):
        with sqlite3.connect(self.registry) as db:
            db.execute('ALTER TABLE material_evidence ADD COLUMN source_work_url TEXT')
            db.execute('ALTER TABLE material_evidence ADD COLUMN source_title TEXT')
            db.execute('INSERT INTO material_evidence VALUES(?,?,?,?,?,?,?,?,?)',
                       (1, 1, None, '{legacy unparsed claim', 'pending', 'legacy.scope', 7,
                        'https://example.org/legacy-source', 'Existing legacy source'))
        queue = self.queue()
        compact = queue.next('legacy-session', compact=True)
        evidence = compact['record_context']['existing_evidence'][0]
        self.assertEqual(evidence['source_title'], 'Existing legacy source')
        self.assertEqual(evidence['source_id'], 7)
        self.assertEqual(evidence['source_urls'], ['https://example.org/legacy-source'])
        self.assertIn('expand full evidence', evidence['claim_json_status'])
        full = queue.next('legacy-session')
        self.assertEqual(full['claim_token'], compact['claim_token'])
        self.assertEqual(full['record_context']['existing_evidence'][0]['claim_json'], '{legacy unparsed claim')

    def test_next_compact_cli_uses_isolated_queue_and_keeps_default_full_view(self):
        output = io.StringIO()
        with patch.object(module, 'ContentQueue', side_effect=lambda **kw: ContentQueue(self.root, self.data, **kw)), \
                patch.object(sys, 'argv', ['mineral-content-queue.py', 'next', '--worker', 'cli-session', '--compact']), \
                redirect_stdout(output):
            self.assertEqual(module.main(), 0)
        compact = json.loads(output.getvalue())
        self.assertEqual(compact['record_context']['existing_evidence_count'], 0)
        self.assertTrue(compact['research_allowed'])
        full = self.queue().next('cli-session')
        self.assertEqual(full['claim_token'], compact['claim_token'])
        self.assertNotIn('existing_evidence_is_summary', full['record_context'])
        self.assertEqual(full['record_context']['existing_evidence'], [])

    def test_expiry_never_automatically_reassigns_a_live_review(self):
        queue = self.queue()
        first = queue.next('session-a')
        queue.db.execute('UPDATE jobs SET lease_until=? WHERE public_id=?',
                         (time.time() - 1, first['public_id']))
        resumed = queue.next('session-a')
        self.assertTrue(resumed['stale'])
        self.assertEqual(first['claim_token'], resumed['claim_token'])
        self.assertNotEqual(first['public_id'], queue.next('session-b')['public_id'])
        with self.assertRaises(ValueError):
            queue.requeue(first['public_id'], 'Reviewer may still be running', False)
        queue.requeue(first['public_id'], 'Verified no reviewer remains active', True)
        replacement = queue.next('session-c')
        self.assertEqual(replacement['public_id'], first['public_id'])
        self.assertNotEqual(replacement['claim_token'], first['claim_token'])
        with self.assertRaises(ValueError):
            queue.heartbeat('session-a', first['claim_token'])

    def test_heartbeat_and_submission_enforce_worker_and_token(self):
        queue = self.queue()
        job = queue.next('session-a')
        path = self.review_file(job)
        for worker, token in [('session-b', job['claim_token']),
                              ('session-a', 'made-up-token')]:
            with self.subTest(worker=worker, token=token):
                with self.assertRaises(ValueError):
                    queue.heartbeat(worker, token)
                with self.assertRaises(ValueError):
                    queue.submit(worker, token, path)
        queue.heartbeat('session-a', job['claim_token'])
        self.assertFalse(queue.next('session-a')['stale'])
        self.assertEqual(self.state(queue, job['public_id'])['status'], 'in_progress')

    def test_uncertain_result_blocks_worker_but_confirmed_failure_is_a_hold(self):
        queue = self.queue()
        first = queue.next('session-a')
        queue.fail('session-a', first['claim_token'], 'Source capture status unknown', 'uncertain')
        self.assertEqual(queue.next('session-a')['public_id'], first['public_id'])
        self.assertEqual(self.state(queue, first['public_id'])['status'], 'uncertain')
        queue.requeue(first['public_id'], 'Capture confirmed inactive; all work retained', True)
        for kind in ('failed', 'needs_research'):
            job = queue.next('session-a')
            queue.fail('session-a', job['claim_token'], 'Requires further source access', kind)
            self.assertNotEqual(queue.next('session-a')['public_id'], job['public_id'])
            self.assertNotEqual(queue.next('session-b')['public_id'], job['public_id'])
        counts = queue.status()['counts']
        self.assertEqual(counts['failed'], 1)
        self.assertEqual(counts['needs_research'], 1)

    def test_submission_rejects_wrong_identity_and_unbound_scientific_claims(self):
        queue = self.queue()
        job = queue.next('session-a')
        original = self.review(job['public_id'])
        variants = []
        wrong_id = copy.deepcopy(original)
        wrong_id['material_public_id'] = self.ids[1]
        variants.append(wrong_id)
        wrong_name = copy.deepcopy(original)
        wrong_name['canonical_name'] = 'A different mineral'
        variants.append(wrong_name)
        unknown_source = copy.deepcopy(original)
        unknown_source['claims'][0]['source_url'] = 'https://example.org/not-inspected'
        variants.append(unknown_source)
        unknown_question = copy.deepcopy(original)
        unknown_question['claims'][0]['question_key'] = 'invented.unknown'
        variants.append(unknown_question)
        duplicate = copy.deepcopy(original)
        duplicate['claims'].append(copy.deepcopy(duplicate['claims'][0]))
        variants.append(duplicate)
        for index, value in enumerate(variants):
            with self.subTest(case=index):
                with self.assertRaises(ValueError):
                    queue.submit('session-a', job['claim_token'], self.review_file(job, value))
                self.assertEqual(self.state(queue, job['public_id'])['status'], 'in_progress')

    def test_submission_keeps_immutable_bytes_and_frees_worker_for_next_record(self):
        queue = self.queue()
        job = queue.next('session-a')
        path = self.review_file(job)
        original_bytes = path.read_bytes()
        submitted = queue.submit('session-a', job['claim_token'], path)
        self.assertEqual(self.state(queue, job['public_id'])['status'], 'submitted')
        queue.submit('session-a', job['claim_token'], path)
        changed = self.review(job['public_id'])
        changed['claims'][0]['value'] = 'red'
        self.review_file(job, changed)
        with self.assertRaises(ValueError):
            queue.submit('session-a', job['claim_token'], path)
        self.assertEqual(Path(submitted['submission_path']).read_bytes(), original_bytes)
        self.assertEqual(self.state(queue, job['public_id'])['status'], 'submitted')
        self.assertNotEqual(queue.next('session-a')['public_id'], job['public_id'])

    def test_queue_operations_never_modify_registry_or_public_catalog(self):
        before = [hashlib.sha256(path.read_bytes()).hexdigest()
                  for path in (self.registry, self.source, self.manifest_path)]
        queue = self.queue()
        job = queue.next('session-a')
        queue.status()
        queue.heartbeat('session-a', job['claim_token'])
        queue.submit('session-a', job['claim_token'], self.review_file(job))
        queue.next('session-a')
        after = [hashlib.sha256(path.read_bytes()).hexdigest()
                 for path in (self.registry, self.source, self.manifest_path)]
        self.assertEqual(before, after)
        # A queue operation must not quietly copy the registry under another
        # filename. Check SQLite headers rather than relying on extensions.
        databases = set()
        for path in self.data.rglob('*'):
            if path.is_file():
                with path.open('rb') as content:
                    if content.read(16) == b'SQLite format 3\x00':
                        databases.add(path.resolve())
        self.assertEqual(databases, {self.registry.resolve(),
                                    (queue.work / 'queue.sqlite3').resolve()})

    def test_completion_reads_actual_saved_claims_history_draft_and_evidence(self):
        queue = self.queue()
        job = queue.next('session-a')
        queue.submit('session-a', job['claim_token'], self.review_file(job))
        receipt_path, _, _, _ = self.saved_review(job)
        before = hashlib.sha256(self.registry.read_bytes()).hexdigest()
        queue.complete(job['public_id'], receipt_path)
        self.assertEqual(self.state(queue, job['public_id'])['status'], 'complete')
        queue.complete(job['public_id'], receipt_path)
        self.assertEqual(before, hashlib.sha256(self.registry.read_bytes()).hexdigest())

    def test_successful_receipt_cannot_hide_missing_or_different_saved_evidence(self):
        for corruption in ('observations', 'history', 'draft', 'evidence', 'ingestion'):
            with self.subTest(corruption=corruption):
                queue = self.queue()
                worker = 'session-' + corruption
                job = queue.next(worker)
                queue.submit(worker, job['claim_token'], self.review_file(job))
                # Use fresh record IDs and replace this fixture's prior save rows.
                with sqlite3.connect(self.registry) as db:
                    db.execute('DELETE FROM material_evidence')
                    db.execute('DELETE FROM ingestion_runs')
                receipt_path, _, _, _ = self.saved_review(job, batch='batch-' + corruption)
                with sqlite3.connect(self.registry) as db:
                    if corruption in ('evidence', 'ingestion'):
                        db.execute('DELETE FROM ' + ('material_evidence' if corruption == 'evidence'
                                                    else 'ingestion_runs'))
                    else:
                        props = json.loads(db.execute(
                            'SELECT properties_json FROM materials WHERE public_id=?',
                            (job['public_id'],)).fetchone()[0])
                        if corruption == 'observations':
                            props['publication_observations'][0]['value'] = 'a different value'
                        elif corruption == 'history':
                            props['publication_review_history'][0]['remaining_gaps'] = []
                        else:
                            props['research_draft_descriptions'] = []
                        db.execute('UPDATE materials SET properties_json=? WHERE public_id=?',
                                   (json.dumps(props), job['public_id']))
                with self.assertRaises(ValueError):
                    queue.complete(job['public_id'], receipt_path)
                self.assertEqual(self.state(queue, job['public_id'])['status'], 'submitted')

    def test_receipt_checks_payload_hash_and_success_guarantees(self):
        queue = self.queue()
        job = queue.next('session-a')
        queue.submit('session-a', job['claim_token'], self.review_file(job))
        receipt_path, receipt, payload_path, payload = self.saved_review(job)
        for key, value in [('status', 'failed'), ('payload_sha256', '0' * 64),
                           ('completed_material_ids', [self.ids[1]]),
                           ('foreign_key_violations', 1), ('FTS_integrity', 'failed'),
                           ('conflicting_primary_targets', 1),
                           ('public_research_and_catalog_unchanged', False)]:
            with self.subTest(key=key):
                changed = dict(receipt, **{key: value})
                self.write_json(receipt_path, changed)
                with self.assertRaises(ValueError):
                    queue.complete(job['public_id'], receipt_path)
                self.assertEqual(self.state(queue, job['public_id'])['status'], 'submitted')
        self.write_json(receipt_path, receipt)
        changed = copy.deepcopy(payload)
        changed['records'][0]['claims'][0]['value'] = 'artifact tampered after save'
        self.write_json(payload_path, changed)
        with self.assertRaises(ValueError):
            queue.complete(job['public_id'], receipt_path)

    def test_version_two_receipt_requires_changed_record_checks_and_checkpoint_audit(self):
        queue = self.queue()
        job = queue.next('version-two-session')
        queue.submit('version-two-session', job['claim_token'], self.review_file(job))
        receipt_path, receipt, _, _ = self.saved_review(job)
        for field in ('integrity', 'foreign_key_violations', 'FTS_integrity', 'conflicting_primary_targets'):
            receipt.pop(field)
        receipt.update({
            'receipt_version': 2, 'validation_scope': 'changed_records',
            'batch_checks': {'record_preservation': 'ok', 'evidence_links': 'ok',
                             'foreign_keys': 'enforced', 'search_updates': 'ok',
                             'cod_assignments': 'ok'},
            'checkpoint_audit': {'integrity': 'ok', 'foreign_key_violations': 0,
                                 'FTS_integrity': 'ok', 'conflicting_primary_targets': 0,
                                 'checked_at': '2026-10-17T00:00:00Z'}})
        with sqlite3.connect(self.registry) as db:
            db.row_factory = sqlite3.Row
            row = dict(db.execute('SELECT * FROM materials WHERE public_id=?',
                                  (job['public_id'],)).fetchone())
        row_bytes = json.dumps(row, ensure_ascii=False, sort_keys=True,
                               separators=(',', ':'), allow_nan=False).encode()
        receipt['results'][0]['material_sha256'] = hashlib.sha256(row_bytes).hexdigest()
        with sqlite3.connect(self.registry) as db:
            metadata = json.loads(db.execute('SELECT metadata_json FROM ingestion_runs WHERE id=1').fetchone()[0])
            metadata.update({key: copy.deepcopy(receipt[key])
                             for key in ('validation_scope', 'batch_checks', 'results')})
            db.execute('UPDATE ingestion_runs SET metadata_json=? WHERE id=1', (json.dumps(metadata),))
        variants = []
        changed = copy.deepcopy(receipt)
        changed['validation_scope'] = 'unchecked'
        variants.append(('scope', changed))
        for check in receipt['batch_checks']:
            changed = copy.deepcopy(receipt)
            changed['batch_checks'][check] = 'failed'
            variants.append(('batch-' + check, changed))
        for check, value in (('integrity', 'failed'), ('foreign_key_violations', 1),
                             ('FTS_integrity', 'failed'), ('conflicting_primary_targets', 1),
                             ('checked_at', None)):
            changed = copy.deepcopy(receipt)
            changed['checkpoint_audit'][check] = value
            variants.append(('checkpoint-' + check, changed))
        changed = copy.deepcopy(receipt)
        changed['results'][0].pop('material_sha256')
        variants.append(('missing-material-hash', changed))
        for case, changed in variants:
            with self.subTest(case=case):
                self.write_json(receipt_path, changed)
                with self.assertRaises(ValueError):
                    queue.complete(job['public_id'], receipt_path)
                self.assertEqual(self.state(queue, job['public_id'])['status'], 'submitted')
        self.write_json(receipt_path, receipt)
        for field in ('validation_scope', 'batch_checks', 'results'):
            with self.subTest(ingestion_metadata=field):
                changed_metadata = copy.deepcopy(metadata)
                changed_metadata.pop(field)
                with sqlite3.connect(self.registry) as db:
                    db.execute('UPDATE ingestion_runs SET metadata_json=? WHERE id=1',
                               (json.dumps(changed_metadata),))
                with self.assertRaisesRegex(ValueError, 'Ingestion validation metadata differs'):
                    queue.complete(job['public_id'], receipt_path)
                self.assertEqual(self.state(queue, job['public_id'])['status'], 'submitted')
        with sqlite3.connect(self.registry) as db:
            db.execute('UPDATE ingestion_runs SET metadata_json=? WHERE id=1', (json.dumps(metadata),))
        queue.complete(job['public_id'], receipt_path)
        self.assertEqual(self.state(queue, job['public_id'])['status'], 'complete')

    def test_foreign_scientific_work_cannot_complete_submitted_reservation(self):
        queue = self.queue()
        job = queue.next('session-a')
        queue.submit('session-a', job['claim_token'], self.review_file(job))
        unrelated = self.review(job['public_id'])
        unrelated['claims'][0]['claim_id'] = 'an-unrelated-review-claim'
        receipt_path, _, _, _ = self.saved_review(job, unrelated)
        with self.assertRaises(ValueError):
            queue.complete(job['public_id'], receipt_path)
        self.assertEqual(self.state(queue, job['public_id'])['status'], 'submitted')

    def test_stale_receipt_cannot_complete_a_new_claim_with_the_same_scientific_ids(self):
        queue = self.queue()
        job = queue.next('session-a')
        queue.submit('session-a', job['claim_token'], self.review_file(job))
        receipt_path, receipt, payload_path, payload = self.saved_review(job)
        original_binding = copy.deepcopy(payload['records'][0]['content_queue_claim'])
        bindings = [None, dict(original_binding, claim_token='stale-former-claim'),
                    dict(original_binding, submission_sha256='0' * 64)]
        for binding in bindings:
            with self.subTest(binding=binding):
                payload['records'][0]['content_queue_claim'] = binding
                self.write_json(payload_path, payload)
                changed_hash = hashlib.sha256(payload_path.read_bytes()).hexdigest()
                receipt['payload_sha256'] = changed_hash
                self.write_json(receipt_path, receipt)
                with sqlite3.connect(self.registry) as db:
                    db.execute('UPDATE ingestion_runs SET metadata_json=? WHERE id=1',
                               (json.dumps({'payload_sha256': changed_hash,
                                            'coordinator_verified': True,
                                            'public_ingestion_allowed': False}),))
                with self.assertRaisesRegex(ValueError, 'bound to this queue submission'):
                    queue.complete(job['public_id'], receipt_path)
                self.assertEqual(self.state(queue, job['public_id'])['status'], 'submitted')

    def test_seed_imports_verified_first_pass_but_never_partial_observations(self):
        saved_packet = {'public_id': self.ids[0], 'claim_token': 'historical-coordinator'}
        self.review_file(saved_packet)
        receipt_path, _, payload_path, _ = self.saved_review(saved_packet)
        checkpoint = self.data / module.REVIEW_ROOT / 'overnight-2026-10-09/checkpoint-01'
        checkpoint.mkdir(parents=True, exist_ok=True)
        (checkpoint / receipt_path.name).write_bytes(receipt_path.read_bytes())
        (checkpoint / payload_path.name).write_bytes(payload_path.read_bytes())
        with sqlite3.connect(self.registry) as db:
            db.execute('UPDATE materials SET properties_json=? WHERE public_id=?', (
                json.dumps({'publication_observations': [{'claim_id': 'older-partial-note'}]}),
                self.ids[1]))
        self.write_json(self.data / module.REVIEW_ROOT /
                        'resumed-2026-10-08/broader-record-review-queue.json', {'records': [
                            {'material_public_id': self.ids[0],
                             'state': 'reviewed_with_remaining_gaps',
                             'latest_checkpoint': 'checkpoint-01', 'ingestion_run_id': 1},
                            {'material_public_id': self.ids[1], 'state': 'prior_partial_research'},
                        ]})
        queue = self.queue()
        self.assertEqual(queue.status()['counts']['complete'], 1)
        job = queue.next('session-a')
        self.assertEqual(job['public_id'], self.ids[1])
        self.assertEqual(job['record_context']['material']['properties_json']
                         ['publication_observations'][0]['claim_id'], 'older-partial-note')

    def test_catalog_release_change_refuses_migration_without_resetting_claims(self):
        queue = self.queue()
        job = queue.next('session-a')
        self.manifest['release_id'] = 'fixture-release-2'
        self.write_json(self.manifest_path, self.manifest)
        with self.assertRaisesRegex(ValueError, 'Catalog release changed'):
            ContentQueue(self.root, self.data)
        self.assertEqual(queue.next('session-a')['claim_token'], job['claim_token'])
        self.assertEqual(self.state(queue, job['public_id'])['claim_count'], 1)

    def test_submission_rejects_invalid_json_outside_paths_and_symlinks(self):
        queue = self.queue()
        job = queue.next('session-a')
        path = self.review_file(job)
        outside = self.write_json(self.root / 'outside-private-data.json', self.review(job['public_id']))
        link = self.data / 'linked-review.json'
        link.symlink_to(path)
        for artifact in (outside, link):
            with self.subTest(artifact=str(artifact)):
                with self.assertRaises(ValueError):
                    queue.submit('session-a', job['claim_token'], artifact)
                self.assertEqual(self.state(queue, job['public_id'])['status'], 'in_progress')
        path.write_text('{not valid JSON')
        with self.assertRaises(ValueError):
            queue.submit('session-a', job['claim_token'], path)
        self.review_file(job)
        queue.submit('session-a', job['claim_token'], path)
        with self.assertRaises(ValueError):
            queue.submit('session-a', job['claim_token'], link)

    def test_modified_submission_snapshot_prevents_completion(self):
        queue = self.queue()
        job = queue.next('session-a')
        submitted = queue.submit('session-a', job['claim_token'], self.review_file(job))
        receipt_path, _, _, _ = self.saved_review(job)
        changed = self.review(job['public_id'])
        changed['claims'][0]['value'] = 'tampered immutable snapshot'
        self.write_json(Path(submitted['submission_path']), changed)
        with self.assertRaisesRegex(ValueError, 'submission hash mismatch'):
            queue.complete(job['public_id'], receipt_path)
        self.assertEqual(self.state(queue, job['public_id'])['status'], 'submitted')

    def test_zero_findings_first_pass_keeps_explicit_source_access_and_gaps(self):
        queue = self.queue()
        job = queue.next('session-a')
        review = self.review(job['public_id'])
        review['claims'] = []
        review['review_note'] = 'Original description located; numerical tables not accessible.'
        review['editorial_draft']['text'] = ''
        review['remaining_gaps'] = ['Original physical-property tables could not be inspected.']
        queue.submit('session-a', job['claim_token'], self.review_file(job, review))
        receipt_path, _, _, _ = self.saved_review(job, review)
        result = queue.complete(job['public_id'], receipt_path)
        self.assertEqual(result['observations'], 0)
        self.assertEqual(self.state(queue, job['public_id'])['status'], 'complete')

    def test_coordinator_can_correct_values_without_losing_submission_binding(self):
        queue = self.queue()
        job = queue.next('session-a')
        submitted = queue.submit('session-a', job['claim_token'], self.review_file(job))
        accepted = self.review(job['public_id'])
        accepted['claims'][0]['value'] = 'pale green'
        accepted['claims'][0]['qualification'] = 'Color corrected after independent original-page check.'
        receipt_path, _, _, _ = self.saved_review(job, accepted)
        result = queue.complete(job['public_id'], receipt_path)
        self.assertEqual(result['observations'], 1)
        original = json.loads(Path(submitted['submission_path']).read_bytes())
        self.assertEqual(original['claims'][0]['value'], 'green')

    def test_deferred_claims_require_exact_structured_identity_not_prose_or_prefix(self):
        queue = self.queue()
        for representation in ('prose', 'prefix_collision', 'structured_claim'):
            with self.subTest(representation=representation):
                worker = 'session-' + representation
                job = queue.next(worker)
                original = self.review(job['public_id'])
                queue.submit(worker, job['claim_token'], self.review_file(job, original))
                accepted = copy.deepcopy(original)
                accepted['claims'] = []
                claim = original['claims'][0]
                if representation == 'prose':
                    accepted['remaining_candidates'] = [
                        {'note': 'Only an incidental mention of ' + claim['claim_id']}]
                elif representation == 'prefix_collision':
                    accepted['deferred_claims'] = [dict(claim, claim_id=claim['claim_id'] + '0')]
                else:
                    accepted['deferred_claims'] = [dict(claim, reason='Recheck specimen context')]
                with sqlite3.connect(self.registry) as db:
                    db.execute('DELETE FROM material_evidence')
                    db.execute('DELETE FROM ingestion_runs')
                receipt_path, _, _, _ = self.saved_review(job, accepted,
                                                          batch='batch-' + representation)
                if representation == 'structured_claim':
                    result = queue.complete(job['public_id'], receipt_path)
                    self.assertEqual(result['observations'], 0)
                    self.assertEqual(self.state(queue, job['public_id'])['status'], 'complete')
                else:
                    with self.assertRaisesRegex(ValueError, 'not accepted or retained'):
                        queue.complete(job['public_id'], receipt_path)
                    self.assertEqual(self.state(queue, job['public_id'])['status'], 'submitted')

    def test_compressed_catalog_initializes_without_writing_public_files_and_resumes(self):
        compressed = self.compressed_catalog()
        before = {path.name: path.read_bytes() for path in compressed.parent.iterdir()}
        job = self.queue().next('session-a')
        resumed = self.queue().next('session-a')
        self.assertEqual(job['public_id'], self.ids[0])
        self.assertEqual(job['claim_token'], resumed['claim_token'])
        self.assertEqual(job['record_context']['material'], resumed['record_context']['material'])
        self.assertFalse(self.source.exists())
        self.assertEqual(before, {path.name: path.read_bytes()
                                  for path in compressed.parent.iterdir()})

    def test_compressed_catalog_must_match_declared_uncompressed_hash(self):
        compressed = self.compressed_catalog()
        original = compressed.read_bytes()
        self.manifest['database']['sha256'] = 'sha256:' + '0' * 64
        self.write_json(self.manifest_path, self.manifest)
        with self.assertRaisesRegex(ValueError, 'Catalog hash mismatch'):
            ContentQueue(self.root, self.data)
        self.assertEqual(compressed.read_bytes(), original)
        self.assertFalse(self.source.exists())

    def test_compressed_catalog_rejects_stream_exceeding_declared_size(self):
        compressed = self.compressed_catalog()
        original = compressed.read_bytes()
        self.manifest['database']['bytes'] = 1
        self.write_json(self.manifest_path, self.manifest)
        with self.assertRaisesRegex(ValueError, 'exceeds declared size'):
            ContentQueue(self.root, self.data)
        self.assertEqual(compressed.read_bytes(), original)
        self.assertFalse(self.source.exists())


if __name__ == '__main__':
    unittest.main()
