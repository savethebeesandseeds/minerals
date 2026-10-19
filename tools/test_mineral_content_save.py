"""Exercise independent content saves with isolated SQLite/FTS5 fixtures only.

Run inside minerals-admin. No test opens the real queue, mineral database,
publication history, source captures or public catalogue.
"""

from concurrent.futures import ProcessPoolExecutor
import hashlib
import importlib.util
import json
import multiprocessing
from pathlib import Path
import sqlite3
import tempfile
import time
import unittest
from unittest.mock import patch


def load_tool(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


queue_module = load_tool('content_queue_save_fixture', 'mineral-content-queue.py')
ContentQueue = queue_module.ContentQueue


def independent_save(arguments):
    root, data, worker, start_at = arguments
    saver = load_tool('independent_content_save', 'mineral-content-save.py')
    queue = ContentQueue(root, data, lease_seconds=60)
    try:
        delay = start_at - time.time()
        if delay > 0:
            time.sleep(delay)
        return saver.save_worker(queue, worker, confirm_reviewed=True)
    finally:
        queue.close()


def fail_before_commit(stage):
    if stage == 'before_commit':
        raise RuntimeError('fixture interruption before commit')


def fail_after_commit(stage):
    if stage == 'after_commit':
        raise RuntimeError('fixture interruption after commit')


class ContentSaveTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='minerals-content-save-test-')
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.data = self.root / 'private-data'
        self.data.mkdir()
        self.database = self.data / 'minerals.db'
        self.ids = ['mat_' + f'{number:032x}' for number in range(3)]
        self.existing_url = 'https://example.org/prior-publication'
        self.shared_url = 'https://example.org/shared-new-publication'
        self.prior_observation = {
            'claim_id': 'prior-observation', 'question_key': 'appearance.color',
            'value': 'blue', 'source_url': self.existing_url, 'locator': 'p.1',
            'qualification': 'Previously reviewed specimen.', 'review_batch': 'prior-batch',
            'review_status': 'reviewed', 'custom_original_field': {'keep': [1, 2, 3]}}
        self.prior_properties = {
            'publication_observations': [self.prior_observation],
            'publication_review_history': [{'review_batch': 'prior-batch', 'original_note': 'Keep this note.'}],
            'research_draft_descriptions': [{'text': 'Previous private draft.', 'review_batch': 'prior-batch'}],
            'public_research': {'description': 'Already selected public text.', 'claim_ids': ['prior-observation']},
            'protected_ima_identity': {'ima_id': 'IMA2000-001', 'approved_formula': 'SiO2'},
            'unrelated_property': {'valuable_candidates': ['unresolved comparison']}}
        with sqlite3.connect(self.database) as db:
            db.execute('PRAGMA foreign_keys=ON')
            db.executescript('''
                CREATE TABLE materials(
                    id INTEGER PRIMARY KEY, public_id TEXT UNIQUE, slug TEXT UNIQUE,
                    record_type TEXT, canonical_name TEXT, formula TEXT, description TEXT,
                    properties_json TEXT CHECK(json_valid(properties_json)),
                    identifiers_json TEXT, synonyms_json TEXT, nomenclature_status TEXT,
                    is_valid_species INTEGER, source_kind TEXT, license_spdx TEXT,
                    publication_status TEXT, search_text TEXT, updated_at TEXT);
                CREATE TABLE evidence_sources(
                    id INTEGER PRIMARY KEY AUTOINCREMENT, canonical_url TEXT UNIQUE,
                    title TEXT, publisher TEXT, license_spdx TEXT, retrieved_at TEXT,
                    content_hash TEXT, metadata_json TEXT CHECK(json_valid(metadata_json)),
                    created_at TEXT DEFAULT 'fixture', updated_at TEXT DEFAULT 'fixture');
                CREATE TABLE material_evidence(
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    material_id INTEGER REFERENCES materials(id),
                    source_id INTEGER REFERENCES evidence_sources(id), claim_scope TEXT,
                    claim_json TEXT CHECK(json_valid(claim_json)), confidence REAL,
                    review_status TEXT, source_title TEXT, source_publisher TEXT,
                    source_license_spdx TEXT, source_retrieved_at TEXT, source_content_hash TEXT,
                    source_attribution_party TEXT, source_work_title TEXT, source_work_url TEXT,
                    source_license_url TEXT, source_changes_notice TEXT,
                    source_no_endorsement_notice TEXT, source_derived_output_license_spdx TEXT,
                    dataset_key TEXT, source_release_id TEXT,
                    created_at TEXT DEFAULT 'fixture', updated_at TEXT DEFAULT 'fixture',
                    UNIQUE(material_id,source_id,claim_scope));
                CREATE TABLE ingestion_runs(
                    id INTEGER PRIMARY KEY AUTOINCREMENT, source_label TEXT, status TEXT,
                    item_count INTEGER, imported_count INTEGER, evidence_count INTEGER,
                    started_at TEXT DEFAULT 'fixture', completed_at TEXT,
                    metadata_json TEXT CHECK(json_valid(metadata_json)));
                CREATE TABLE retained_private_notes(id INTEGER PRIMARY KEY, note TEXT, original_bytes BLOB);
                CREATE TABLE private_source_labels(code TEXT PRIMARY KEY, note TEXT) WITHOUT ROWID;
                CREATE VIRTUAL TABLE material_search USING fts5(
                    canonical_name,formula,description,search_text,
                    content='materials',content_rowid='id');
                CREATE TRIGGER materials_search_insert AFTER INSERT ON materials BEGIN
                    INSERT INTO material_search(rowid,canonical_name,formula,description,search_text)
                    VALUES(new.id,new.canonical_name,new.formula,new.description,new.search_text);
                END;
                CREATE TRIGGER materials_search_update AFTER UPDATE ON materials BEGIN
                    INSERT INTO material_search(material_search,rowid,canonical_name,formula,description,search_text)
                    VALUES('delete',old.id,old.canonical_name,old.formula,old.description,old.search_text);
                    INSERT INTO material_search(rowid,canonical_name,formula,description,search_text)
                    VALUES(new.id,new.canonical_name,new.formula,new.description,new.search_text);
                END;
            ''')
            for index, public_id in enumerate(self.ids):
                properties = self.prior_properties if index == 0 else {'unrelated_property': {'mineral': index}}
                db.execute('INSERT INTO materials VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)', (
                    index + 1, public_id, f'mineral-{index}', 'mineral', f'Mineral {index:02}', 'SiO2',
                    'Existing public description', json.dumps(properties),
                    json.dumps({'ima_id': f'IMA2000-{index:03}'}), '["Original alias"]', 'approved',
                    1, 'protected_ima_import', 'NOASSERTION', 'published', 'Original search text', 'fixture'))
            db.execute('INSERT INTO evidence_sources VALUES(?,?,?,?,?,?,?,?,?,?)', (
                1, self.existing_url, 'Preserved original source title', 'Original publisher',
                'CC-BY-4.0', 'original-retrieval', 'sha256:original',
                json.dumps({'immutable_original_metadata': {'source_snapshot': ['retain', 'all']}}),
                'original-created', 'original-updated'))
            db.execute('INSERT INTO material_evidence('
                       'id,material_id,source_id,claim_scope,claim_json,confidence,review_status,'
                       'source_title,source_publisher,source_license_spdx,dataset_key,source_release_id) '
                       'VALUES(?,?,?,?,?,?,?,?,?,?,?,?)', (
                           11, 1, 1, 'appearance.color', json.dumps({'value': 'blue',
                               'observation_ids': ['prior-observation'], 'original_snapshot': {'keep': True}}),
                           0.9, 'verified', 'Prior evidence title', 'Original publisher', 'CC-BY-4.0',
                           'old-dataset', 'prior-batch'))
            db.execute('INSERT INTO retained_private_notes VALUES(?,?,?)',
                       (9, 'Unrelated private record', b'\x00valuable\xffsource\x00'))
            db.execute('INSERT INTO private_source_labels VALUES(?,?)',
                       ('source-label', 'Preserved WITHOUT ROWID source metadata'))
        public = self.root / 'public-catalog'
        public.mkdir()
        self.catalog = public / 'catalog.sqlite3'
        with sqlite3.connect(self.catalog) as db:
            db.execute('CREATE TABLE minerals(public_id TEXT,canonical_name TEXT,formula TEXT)')
            db.executemany('INSERT INTO minerals VALUES(?,?,?)',
                           [(identifier, f'Mineral {index:02}', 'SiO2') for index, identifier in enumerate(self.ids)])
        self.write_json(public / 'catalog-manifest.json', {
            'release_id': 'save-fixture-v1', 'mineral_count': len(self.ids),
            'database': {'path': self.catalog.name, 'sha256': 'sha256:' + self.hash(self.catalog)}})
        self.write_json(self.root / 'schemas/mineral-record-questionnaire-v1.json', {
            'modules': {'appearance': {'questions': {'appearance.color': {'lifecycle': 'active'}}}}})
        app = self.root / 'public-app'
        app.mkdir()
        (app / 'index.html').write_text('<!doctype html><title>Unchanged public app</title>')
        self.capture = self.data / 'captures/source.pdf'
        self.capture.parent.mkdir()
        self.capture.write_bytes(b'%PDF-1.4\nFixture original publication capture.\n%%EOF\n')
        self.saver = load_tool('content_save_under_test', 'mineral-content-save.py')
        self.queue = ContentQueue(self.root, self.data, lease_seconds=60)
        self.addCleanup(self.queue.close)
        self.public_before = self.public_state()

    @staticmethod
    def hash(path):
        return hashlib.sha256(path.read_bytes()).hexdigest()

    @staticmethod
    def write_json(path, value):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n')
        return path

    def public_state(self):
        return {str(path.relative_to(self.root)): path.read_bytes()
                for parent in ('public-app', 'public-catalog')
                for path in (self.root / parent).rglob('*') if path.is_file()}

    def database_state(self):
        with sqlite3.connect(self.database) as db:
            db.execute('BEGIN')
            tables = [row[0] for row in db.execute(
                "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")]
            # FTS5 has WITHOUT ROWID shadow tables; compare every saved value
            # without assuming that these internal tables expose rowid.
            return {table: sorted(db.execute(f'SELECT * FROM "{table}"').fetchall(), key=repr)
                    for table in tables}

    def row(self, table, identifier):
        with sqlite3.connect(self.database) as db:
            db.row_factory = sqlite3.Row
            return dict(db.execute(f'SELECT * FROM "{table}" WHERE id=?', (identifier,)).fetchone())

    def job(self, public_id):
        return dict(self.queue.db.execute('SELECT * FROM jobs WHERE public_id=?', (public_id,)).fetchone())

    def prepare_submission(self, worker, source_url=None, claim_updates=None, review_updates=None):
        packet = self.queue.next(worker)
        review = {
            'material_public_id': packet['public_id'], 'canonical_name': packet['canonical_name'],
            'registry_formula': packet['formula'], 'review_artifact_directory': str(self.capture.parent),
            'review_note': 'Fixture scientific review completed with exact source locators.',
            'sources': [{'url': source_url or self.shared_url, 'title': 'New source-scoped citation',
                         'publisher': 'Fixture journal', 'access': 'Synthetic isolated test capture.',
                         'license_spdx': 'NOASSERTION', 'retrieved_at': '2026-10-18T00:00:00Z',
                         'archive': {'archive_path': str(self.capture), 'sha256': self.hash(self.capture)}}],
            'claims': [{'claim_id': packet['public_id'] + '-new-color', 'question_key': 'appearance.color',
                        'value': 'green', 'source_url': source_url or self.shared_url, 'locator': 'p.4,Table1',
                        'qualification': 'Described study specimen only.',
                        'evidence_level': 'primary_publication_research', 'subject_scope': 'specimen',
                        'source_scope': 'article_study_specimen', 'source_access': 'Reviewed full fixture article'}],
            'editorial_draft': {'text': 'A cited private profile describing green study material.',
                                'supporting_claim_ids': [packet['public_id'] + '-new-color']},
            'remaining_gaps': ['Unmeasured hardness remains unknown.'],
            'deferred_claims': [{'claim_id': packet['public_id'] + '-deferred', 'reason': 'Unresolved OCR.'}],
            'remaining_candidates': [{'source_url': 'https://example.org/future-original',
                                      'qualification': 'Useful unreviewed lead preserved.'}]}
        if claim_updates:
            review['claims'][0].update(claim_updates)
        if review_updates:
            review.update(review_updates)
        self.write_json(Path(packet['review_file']), review)
        submitted = self.queue.submit(worker, packet['claim_token'], packet['review_file'])
        return packet, review, submitted

    def save(self, worker, **options):
        return self.saver.save_worker(self.queue, worker, confirm_reviewed=True, **options)

    def receipt(self, packet):
        return json.loads(Path(self.job(packet['public_id'])['receipt_path']).read_text())

    def checkpoints(self):
        return sorted((self.queue.work / 'database-checkpoints').glob('checkpoint-*/minerals.sqlite3'))

    def assert_shared_checkpoint(self, receipt):
        backup = Path(receipt['backup_path'])
        self.assertEqual(backup.parent.parent, self.queue.work / 'database-checkpoints')
        self.assertRegex(backup.parent.name, r'^checkpoint-[0-9a-f]{32}$')
        self.assertEqual(backup.name, 'minerals.sqlite3')
        self.assertTrue((backup.parent / 'checkpoint.json').is_file())
        self.assertEqual(receipt['backup_policy'], 'shared_daily_checkpoint')
        self.assertEqual(receipt['backup_interval_seconds'], 86400)
        self.assertEqual(receipt['backup_retained_checkpoints'], 2)
        self.assertEqual(self.hash(backup), receipt['backup_sha256'])
        self.assertEqual(list((self.queue.work / 'saves').glob('*/before-commit.sqlite3')), [])

    def assert_saved_once(self, packet, review):
        job = self.job(packet['public_id'])
        self.assertEqual(job['status'], 'complete')
        receipt = json.loads(Path(job['receipt_path']).read_text())
        backup = Path(receipt['backup_path'])
        self.assertEqual(self.hash(backup), receipt['backup_sha256'])
        with sqlite3.connect(backup.as_uri() + '?mode=ro', uri=True) as db:
            self.assertEqual(db.execute('PRAGMA integrity_check').fetchone()[0], 'ok')
            self.assertEqual(db.execute('SELECT count(*) FROM ingestion_runs WHERE source_label=?',
                                        (job['review_batch'],)).fetchone()[0], 0)
            old_properties = json.loads(db.execute('SELECT properties_json FROM materials WHERE public_id=?',
                                                   (packet['public_id'],)).fetchone()[0])
            self.assertTrue({claim['claim_id'] for claim in review['claims']}.isdisjoint(
                {claim['claim_id'] for claim in old_properties.get('publication_observations', [])}))
        with sqlite3.connect(self.database) as db:
            material_id, encoded = db.execute('SELECT id,properties_json FROM materials WHERE public_id=?',
                                              (packet['public_id'],)).fetchone()
            properties = json.loads(encoded)
            for claim in review['claims']:
                saved = [item for item in properties['publication_observations']
                         if item['claim_id'] == claim['claim_id']]
                self.assertEqual(len(saved), 1)
                for key, value in claim.items():
                    self.assertEqual(saved[0][key], value)
                self.assertEqual(saved[0]['review_status'], 'reviewed')
            history = [item for item in properties['publication_review_history']
                       if item['review_batch'] == job['review_batch']]
            self.assertEqual(len(history), 1)
            self.assertEqual(history[0]['remaining_gaps'], review['remaining_gaps'])
            self.assertEqual(history[0]['deferred_claims'], review['deferred_claims'])
            self.assertEqual(history[0]['remaining_candidates'], review['remaining_candidates'])
            self.assertEqual(history[0]['publication_status'], 'private')
            self.assertIs(history[0]['public_ingestion_allowed'], False)
            drafts = [item for item in properties['research_draft_descriptions']
                      if item['review_batch'] == job['review_batch']]
            self.assertEqual(len(drafts), 1)
            self.assertEqual(drafts[0]['text'], review['editorial_draft']['text'])
            self.assertIs(drafts[0]['private_research'], True)
            self.assertEqual(db.execute('SELECT count(*) FROM ingestion_runs WHERE source_label=?',
                                        (job['review_batch'],)).fetchone()[0], 1)
            bundles = db.execute('SELECT claim_json FROM material_evidence WHERE material_id=? '
                                 'AND source_release_id=?', (material_id, job['review_batch'])).fetchall()
            self.assertEqual(len(bundles), 1 if review['claims'] else 0)
            if review['claims']:
                self.assertEqual(json.loads(bundles[0][0])['observation_ids'], [review['claims'][0]['claim_id']])
            self.assertEqual(db.execute('PRAGMA integrity_check').fetchone()[0], 'ok')
            self.assertEqual(db.execute('PRAGMA foreign_key_check').fetchall(), [])
            db.execute("INSERT INTO material_search(material_search,rank) VALUES('integrity-check',1)")
        self.assertEqual(self.public_state(), self.public_before)

    def test_two_independent_sessions_save_disjoint_claims_once_and_preserve_prior_state(self):
        first, first_review, _ = self.prepare_submission('session-a')
        second, second_review, _ = self.prepare_submission('session-b')
        self.assertNotEqual(first['public_id'], second['public_id'])
        before_materials = {identifier: self.row('materials', identifier) for identifier in (1, 2, 3)}
        before_source, before_evidence = self.row('evidence_sources', 1), self.row('material_evidence', 11)
        before_unrelated = self.row('retained_private_notes', 9)
        before_labels = self.database_state()['private_source_labels']
        start_at = time.time() + 0.5
        arguments = [(str(self.root), str(self.data), worker, start_at) for worker in ('session-a', 'session-b')]
        with ProcessPoolExecutor(max_workers=2, mp_context=multiprocessing.get_context('fork')) as pool:
            results = list(pool.map(independent_save, arguments))
        self.assertTrue(all(result['result'] == 'saved' for result in results))
        for packet, review in ((first, first_review), (second, second_review)):
            self.assert_saved_once(packet, review)
            self.assertEqual(self.save(packet['worker'])['result'], 'no_submissions')
        first_receipt, second_receipt = self.receipt(first), self.receipt(second)
        self.assert_shared_checkpoint(first_receipt)
        self.assert_shared_checkpoint(second_receipt)
        self.assertEqual(first_receipt['backup_path'], second_receipt['backup_path'])
        self.assertEqual(first_receipt['backup_sha256'], second_receipt['backup_sha256'])
        self.assertEqual(len(self.checkpoints()), 1)
        self.assertEqual(self.row('evidence_sources', 1), before_source)
        self.assertEqual(self.row('material_evidence', 11), before_evidence)
        self.assertEqual(self.row('retained_private_notes', 9), before_unrelated)
        self.assertEqual(self.database_state()['private_source_labels'], before_labels)
        assigned_ids = {self.row('materials', index)['public_id'] for index in (1, 2, 3)}
        self.assertEqual(assigned_ids, set(self.ids))
        for identifier, original in before_materials.items():
            current = self.row('materials', identifier)
            self.assertEqual({key: value for key, value in current.items() if key not in ('properties_json', 'search_text')},
                             {key: value for key, value in original.items() if key not in ('properties_json', 'search_text')})
            old_properties, new_properties = json.loads(original['properties_json']), json.loads(current['properties_json'])
            for key, value in old_properties.items():
                if key in ('publication_observations', 'publication_review_history', 'research_draft_descriptions'):
                    self.assertEqual(new_properties[key][:len(value)], value)
                else:
                    self.assertEqual(new_properties[key], value)
        with sqlite3.connect(self.database) as db:
            self.assertEqual(db.execute('SELECT count(*) FROM evidence_sources WHERE canonical_url=?',
                                        (self.shared_url,)).fetchone()[0], 1)
            self.assertEqual(db.execute('SELECT count(*) FROM ingestion_runs').fetchone()[0], 2)

    def test_existing_source_and_scope_collision_keeps_original_row_and_citation(self):
        # Prior observations move mineral00 later in the default queue; make an
        # explicit fixture-only priority so this claim exercises its collision.
        self.queue.db.execute('UPDATE jobs SET ordinal=-1 WHERE public_id=?', (self.ids[0],))
        packet, review, _ = self.prepare_submission('collision-session', self.existing_url)
        old_source, old_evidence = self.row('evidence_sources', 1), self.row('material_evidence', 11)
        self.save('collision-session')
        self.assert_saved_once(packet, review)
        self.assertEqual(self.row('evidence_sources', 1), old_source)
        self.assertEqual(self.row('material_evidence', 11), old_evidence)
        properties = json.loads(self.row('materials', 1)['properties_json'])
        self.assertEqual(properties['publication_observations'][0], self.prior_observation)
        with sqlite3.connect(self.database) as db:
            scope, encoded = db.execute('SELECT claim_scope,claim_json FROM material_evidence WHERE material_id=1 AND id<>11').fetchone()
            self.assertNotEqual(scope, 'appearance.color')
            self.assertEqual(json.loads(encoded)['question_key'], 'appearance.color')

    def test_concurrent_retries_for_same_session_commit_only_once(self):
        packet, review, _ = self.prepare_submission('same-session')
        arguments = [(str(self.root), str(self.data), 'same-session', time.time() + 0.5)] * 2
        with ProcessPoolExecutor(max_workers=2, mp_context=multiprocessing.get_context('fork')) as pool:
            results = list(pool.map(independent_save, arguments))
        self.assertEqual(sorted(result['result'] for result in results), ['no_submissions', 'saved'])
        self.assert_saved_once(packet, review)
        with sqlite3.connect(self.database) as db:
            self.assertEqual(db.execute('SELECT count(*) FROM ingestion_runs').fetchone()[0], 1)

    def test_one_worker_batch_saves_several_reviews_in_one_transaction(self):
        first, first_review, _ = self.prepare_submission('batch-session')
        second, second_review, _ = self.prepare_submission('batch-session')
        self.assertNotEqual(first['public_id'], second['public_id'])
        result = self.save('batch-session')
        self.assertEqual(set(result['completed']), {first['public_id'], second['public_id']})
        self.assert_saved_once(first, first_review)
        self.assert_saved_once(second, second_review)
        with sqlite3.connect(self.database) as db:
            self.assertEqual(db.execute('SELECT count(*) FROM ingestion_runs').fetchone()[0], 1)
            self.assertEqual(db.execute('SELECT count(*) FROM evidence_sources WHERE canonical_url=?',
                                        (self.shared_url,)).fetchone()[0], 1)

    def test_zero_claim_first_pass_saves_explicit_gaps_and_private_draft(self):
        packet, review, _ = self.prepare_submission('zero-claim-session', review_updates={
            'claims': [], 'review_note': 'Original source inspected; no new source-supported values found.',
            'editorial_draft': {'text': 'The inspected source does not resolve the remaining property gaps.',
                                'supporting_claim_ids': []},
            'remaining_gaps': ['Hardness and density were absent from the inspected publication.'],
            'deferred_claims': []})
        result = self.save('zero-claim-session')
        self.assertEqual(result['observations_added'], 0)
        receipt = self.receipt(packet)
        self.assertEqual(receipt['validation_scope'], 'changed_records')
        self.assertEqual(receipt['batch_checks']['search_updates'], 'ok')
        self.assertEqual(receipt['results'][0]['observations'], 0)
        self.assertEqual(receipt['results'][0]['evidence_ids'], [])
        self.assert_saved_once(packet, review)

    def test_shared_source_keeps_first_metadata_and_each_review_citation_snapshot(self):
        first, first_review, _ = self.prepare_submission('shared-source-metadata')
        second_source = dict(first_review['sources'][0], title='Second review citation title',
                             publisher='Second review citation publisher')
        second, second_review, _ = self.prepare_submission('shared-source-metadata', review_updates={
            'sources': [second_source]})
        self.save('shared-source-metadata')
        self.assert_saved_once(first, first_review)
        self.assert_saved_once(second, second_review)
        with sqlite3.connect(self.database) as db:
            sources = db.execute('SELECT title,publisher FROM evidence_sources WHERE canonical_url=?',
                                 (self.shared_url,)).fetchall()
            self.assertEqual(sources, [(first_review['sources'][0]['title'],
                                       first_review['sources'][0]['publisher'])])
            for packet, review in ((first, first_review), (second, second_review)):
                snapshot = db.execute('SELECT e.source_title,e.source_publisher FROM material_evidence e '
                                      'JOIN materials m ON m.id=e.material_id '
                                      'WHERE m.public_id=? AND e.source_release_id=?',
                                      (packet['public_id'], self.job(packet['public_id'])['review_batch'])).fetchone()
                self.assertEqual(snapshot, (review['sources'][0]['title'], review['sources'][0]['publisher']))

    def test_capture_hash_failure_rejects_before_database_or_queue_mutation(self):
        packet, _, _ = self.prepare_submission('bad-capture-session')
        before, job_before = self.database_state(), self.job(packet['public_id'])
        self.capture.write_bytes(b'%PDF-1.4\nChanged bytes after immutable submission.')
        with self.assertRaisesRegex(ValueError, '(?i)(hash|capture)'):
            self.save('bad-capture-session')
        self.assertEqual(self.database_state(), before)
        self.assertEqual(self.job(packet['public_id']), job_before)

    def test_busy_writer_returns_without_mutation_so_other_session_can_continue(self):
        packet, review, _ = self.prepare_submission('busy-session')
        before, job_before = self.database_state(), self.job(packet['public_id'])
        with self.saver.save_lock(self.queue) as acquired:
            self.assertTrue(acquired)
            result = self.saver.save_worker(self.queue, 'busy-session', confirm_reviewed=True,
                                           wait_for_lock=False)
            self.assertEqual(result['result'], 'busy')
            self.assertTrue(result['submissions_retained'])
            self.assertEqual(self.database_state(), before)
            self.assertEqual(self.job(packet['public_id']), job_before)
            with self.saver.save_lock(self.queue, wait=False) as still_busy:
                self.assertFalse(still_busy)
        self.save('busy-session')
        self.assert_saved_once(packet, review)

    def test_interruption_before_commit_rolls_back_and_keeps_submission_reserved(self):
        packet, review, _ = self.prepare_submission('before-commit-session')
        before = self.database_state()
        with self.assertRaisesRegex(RuntimeError, 'fixture interruption before commit'):
            self.save('before-commit-session', fault=fail_before_commit)
        self.assertEqual(self.database_state(), before)
        job = self.job(packet['public_id'])
        self.assertEqual(job['status'], 'submitted')
        self.assertEqual(job['token'], packet['claim_token'])
        checkpoints_before_retry = {path: self.hash(path) for path in self.checkpoints()}
        self.assertEqual(len(checkpoints_before_retry), 1)
        self.save('before-commit-session')
        self.assert_saved_once(packet, review)
        self.assertEqual({path: self.hash(path) for path in self.checkpoints()}, checkpoints_before_retry)
        self.assertEqual(Path(self.receipt(packet)['backup_path']), next(iter(checkpoints_before_retry)))

    def test_interruption_after_commit_recovers_queue_without_duplicate_scientific_rows(self):
        packet, review, _ = self.prepare_submission('after-commit-session')
        with self.assertRaisesRegex(RuntimeError, 'fixture interruption after commit'):
            self.save('after-commit-session', fault=fail_after_commit)
        self.assertEqual(self.job(packet['public_id'])['status'], 'submitted')
        committed = self.database_state()
        checkpoints_before_recovery = {path: self.hash(path) for path in self.checkpoints()}
        # Queue recovery has no new scientific work, even when a new daily
        # checkpoint would otherwise be due.
        with patch.object(self.saver, 'stamp', return_value='2099-01-01T00:00:00Z'):
            result = self.save('after-commit-session')
        self.assertEqual(len(result['recovered']), 1)
        self.assert_saved_once(packet, review)
        self.assertEqual(self.database_state(), committed)
        self.assertEqual({path: self.hash(path) for path in self.checkpoints()}, checkpoints_before_recovery)
        self.assertEqual(self.save('after-commit-session')['result'], 'no_submissions')

    def test_empty_save_creates_no_checkpoint_or_batch_artifacts(self):
        before = self.database_state()
        files_before = {path.relative_to(self.data) for path in self.data.rglob('*') if path.is_file()}
        with patch.object(self.saver, 'create_checkpoint',
                          side_effect=AssertionError('Empty saves must not copy the database')):
            result = self.save('idle-session')
        self.assertEqual(result['result'], 'no_submissions')
        self.assertEqual(self.database_state(), before)
        files_after = {path.relative_to(self.data) for path in self.data.rglob('*') if path.is_file()}
        self.assertEqual(files_after - files_before, {Path('pilots/mineral-content-queue-v1/save.lock')})
        self.assertFalse((self.queue.work / 'database-checkpoints').exists())
        self.assertFalse((self.queue.work / 'saves').exists())

    def test_later_worker_reuses_checkpoint_until_the_24_hour_boundary(self):
        first, first_review, _ = self.prepare_submission('daily-first')
        with patch.object(self.saver, 'stamp', return_value='2026-10-18T08:00:00Z'):
            self.save('daily-first')
        first_receipt = self.receipt(first)
        second, second_review, _ = self.prepare_submission('daily-second')
        with patch.object(self.saver, 'stamp', return_value='2026-10-19T07:59:59Z'):
            self.save('daily-second')
        second_receipt = self.receipt(second)
        self.assert_shared_checkpoint(first_receipt)
        self.assert_shared_checkpoint(second_receipt)
        for key in ('backup_path', 'backup_sha256', 'backup_created_at'):
            self.assertEqual(first_receipt[key], second_receipt[key])
        self.assertEqual(first_receipt['backup_created_at'], '2026-10-18T08:00:00Z')
        self.assertEqual(len(self.checkpoints()), 1)
        self.assert_saved_once(first, first_review)
        self.assert_saved_once(second, second_review)

    def test_routine_save_checks_changed_records_without_full_audits_or_checkpoint_hashing(self):
        first, _, _ = self.prepare_submission('routine-checkpoint')
        self.save('routine-checkpoint')
        backup = Path(self.receipt(first)['backup_path'])
        packet, review, _ = self.prepare_submission('routine-next')
        queries = []
        original_connect = sqlite3.connect

        def traced_connection(*args, **kwargs):
            connection = original_connect(*args, **kwargs)
            connection.set_trace_callback(queries.append)
            return connection

        with patch.object(self.saver.sqlite3, 'connect', side_effect=traced_connection), \
                patch.object(self.saver, 'file_hash', wraps=self.saver.file_hash) as hashes:
            self.save('routine-next')
        statements = [' '.join(query.lower().split()) for query in queries]
        for forbidden in ('pragma integrity_check', 'pragma foreign_key_check',
                          "values('integrity-check'", 'with primary_links as',
                          "sum(json_array_length(properties_json"):
            self.assertFalse(any(forbidden in query for query in statements), forbidden)
        hashed_paths = {Path(call.args[0]).resolve() for call in hashes.call_args_list}
        self.assertNotIn(backup.resolve(), hashed_paths)
        self.assertNotIn(self.database.resolve(), hashed_paths)
        receipt = self.receipt(packet)
        self.assertEqual(receipt['receipt_version'], 2)
        self.assertEqual(receipt['validation_scope'], 'changed_records')
        self.assertEqual(receipt['batch_checks'], {
            'record_preservation': 'ok', 'evidence_links': 'ok', 'foreign_keys': 'enforced',
            'search_updates': 'ok', 'cod_assignments': 'ok'})
        self.assertEqual(receipt['checkpoint_audit'], self.receipt(first)['checkpoint_audit'])
        self.assertEqual(receipt['checkpoint_audit']['integrity'], 'ok')
        self.assertEqual(receipt['checkpoint_audit']['foreign_key_violations'], 0)
        self.assertEqual(receipt['checkpoint_audit']['FTS_integrity'], 'ok')
        self.assertEqual(receipt['checkpoint_audit']['conflicting_primary_targets'], 0)
        self.assertTrue(receipt['checkpoint_audit']['checked_at'])
        for removed in ('integrity', 'foreign_key_violations', 'FTS_integrity',
                        'conflicting_primary_targets', 'preserved_unrelated_state_sha256',
                        'private_observation_minerals', 'private_observations'):
            self.assertNotIn(removed, receipt)
        self.assertRegex(receipt['results'][0]['material_sha256'], r'^[0-9a-f]{64}$')
        self.assert_saved_once(packet, review)

    def test_collateral_trigger_cannot_mutate_an_unrelated_table(self):
        packet, _, _ = self.prepare_submission('collateral-trigger')
        with sqlite3.connect(self.database) as db:
            db.executescript('''
                CREATE TRIGGER unexpected_private_note_update AFTER UPDATE ON materials BEGIN
                    UPDATE retained_private_notes SET note='Unwanted collateral mutation' WHERE id=9;
                END;
            ''')
        before, job_before = self.database_state(), self.job(packet['public_id'])
        with self.assertRaises((ValueError, sqlite3.DatabaseError)):
            self.save('collateral-trigger')
        self.assertEqual(self.database_state(), before)
        self.assertEqual(self.job(packet['public_id']), job_before)

    def test_trigger_cannot_change_a_protected_column_of_the_reviewed_material(self):
        packet, _, _ = self.prepare_submission('protected-column-trigger')
        with sqlite3.connect(self.database) as db:
            db.executescript('''
                CREATE TRIGGER unexpected_formula_update AFTER UPDATE OF properties_json ON materials BEGIN
                    UPDATE materials SET formula='Changed protected formula' WHERE id=new.id;
                END;
            ''')
        before, job_before = self.database_state(), self.job(packet['public_id'])
        with self.assertRaises((ValueError, sqlite3.DatabaseError)):
            self.save('protected-column-trigger')
        self.assertEqual(self.database_state(), before)
        self.assertEqual(self.job(packet['public_id']), job_before)

    def test_missing_search_update_trigger_prevents_commit_after_checkpoint_reuse(self):
        self.prepare_submission('search-checkpoint')
        self.save('search-checkpoint')
        packet, _, _ = self.prepare_submission('search-trigger-missing')
        with sqlite3.connect(self.database) as db:
            db.execute('DROP TRIGGER materials_search_update')
        before, job_before = self.database_state(), self.job(packet['public_id'])
        with self.assertRaisesRegex(ValueError, '(?i)(search|fts)'):
            self.save('search-trigger-missing')
        self.assertEqual(self.database_state(), before)
        self.assertEqual(self.job(packet['public_id']), job_before)

    def test_primary_cod_conflicts_with_scalar_or_bundled_prior_assignment_roll_back(self):
        self.write_json(self.root / 'schemas/mineral-record-questionnaire-v1.json', {
            'modules': {'appearance': {'questions': {'appearance.color': {'lifecycle': 'active'}}},
                        'properties': {'questions': {'properties.cod_record_link': {'lifecycle': 'active'}}}}})
        for representation in ('scalar', 'bundled'):
            with self.subTest(representation=representation):
                cod_value = {'cod_id': '9999999', 'primary_assignment': True}
                value = cod_value if representation == 'scalar' else {
                    'reported_observations': [{'question_key': 'properties.cod_record_link', 'value': cod_value}]}
                with sqlite3.connect(self.database) as db:
                    db.execute("DELETE FROM material_evidence WHERE claim_scope='properties.cod_record_link'")
                    db.execute('INSERT INTO material_evidence(material_id,source_id,claim_scope,claim_json) '
                               'VALUES(?,?,?,?)', (1, 1, 'properties.cod_record_link', json.dumps({
                                   'question_key': 'properties.cod_record_link', 'value': value})))
                worker = 'cod-conflict-' + representation
                packet, _, _ = self.prepare_submission(worker, claim_updates={
                    'question_key': 'properties.cod_record_link', 'value': cod_value})
                self.assertNotEqual(packet['public_id'], self.ids[0])
                before, job_before = self.database_state(), self.job(packet['public_id'])
                with self.assertRaisesRegex(ValueError, '(?i)(conflict|cod)'):
                    self.save(worker)
                self.assertEqual(self.database_state(), before)
                self.assertEqual(self.job(packet['public_id']), job_before)

    def test_malformed_primary_cod_identifiers_roll_back_and_keep_submission_reserved(self):
        self.write_json(self.root / 'schemas/mineral-record-questionnaire-v1.json', {
            'modules': {'appearance': {'questions': {'appearance.color': {'lifecycle': 'active'}}},
                        'properties': {'questions': {'properties.cod_record_link': {'lifecycle': 'active'}}}}})
        previous_packet = None
        for cod_id in (True, [], {}, 1.5, ''):
            with self.subTest(cod_id=cod_id):
                if previous_packet is not None:
                    self.queue.requeue(previous_packet['public_id'], 'Reset isolated malformed-input fixture', True)
                packet, _, _ = self.prepare_submission('malformed-cod-session', claim_updates={
                    'question_key': 'properties.cod_record_link',
                    'value': {'cod_id': cod_id, 'primary_assignment': True}})
                before, job_before = self.database_state(), self.job(packet['public_id'])
                with self.assertRaisesRegex(ValueError, '(?i)(cod|identifier)'):
                    self.save('malformed-cod-session')
                self.assertEqual(self.database_state(), before)
                self.assertEqual(self.job(packet['public_id']), job_before)
                self.assertEqual(job_before['status'], 'submitted')
                previous_packet = packet

    def test_daily_rotation_retains_two_checkpoints_and_old_receipt_still_completes(self):
        legacy = self.queue.work / 'saves' / 'legacy-save' / 'before-commit.sqlite3'
        legacy.parent.mkdir(parents=True)
        legacy.write_bytes(b'Legacy backup must remain untouched.\x00\xff')
        legacy_before = legacy.read_bytes()
        receipts = []
        first = None
        for day in range(3):
            worker = 'rotation-' + str(day)
            packet, review, _ = self.prepare_submission(worker)
            if first is None:
                first = packet
            with patch.object(self.saver, 'stamp', return_value=f'2026-10-{18 + day:02}T08:00:00Z'):
                self.save(worker)
            receipt = self.receipt(packet)
            # Legacy backup files are deliberately outside the new retention
            # policy and therefore outside assert_shared_checkpoint's no-new-
            # per-batch-copy assertion.
            self.assertEqual(receipt['backup_policy'], 'shared_daily_checkpoint')
            self.assert_saved_once(packet, review)
            receipts.append(receipt)
            self.assertEqual(len(self.checkpoints()), min(day + 1, 2))
        self.assertEqual(len({receipt['backup_path'] for receipt in receipts}), 3)
        self.assertEqual(receipts[1]['backup_created_at'], '2026-10-19T08:00:00Z')
        self.assertEqual(receipts[2]['backup_created_at'], '2026-10-20T08:00:00Z')
        self.assertFalse(Path(receipts[0]['backup_path']).exists())
        self.assertEqual({str(path) for path in self.checkpoints()},
                         {receipt['backup_path'] for receipt in receipts[1:]})
        self.assertEqual(legacy.read_bytes(), legacy_before)
        self.assertEqual(list((self.queue.work / 'saves').glob('*/before-commit.sqlite3')), [legacy])
        original_job = self.job(first['public_id'])
        completed = self.queue.complete(first['public_id'], original_job['receipt_path'])
        self.assertEqual(completed['result'], 'complete')
        self.assertEqual(self.job(first['public_id']), original_job)

    def test_failed_checkpoint_publication_preserves_database_queue_and_prior_checkpoint(self):
        first, _, _ = self.prepare_submission('checkpoint-created')
        with patch.object(self.saver, 'stamp', return_value='2026-10-18T08:00:00Z'):
            self.save('checkpoint-created')
        packet, review, _ = self.prepare_submission('checkpoint-failure')
        before, job_before = self.database_state(), self.job(packet['public_id'])
        checkpoints_before = {path: self.hash(path) for path in self.checkpoints()}
        original_write_json = self.saver.write_json

        def fail_checkpoint_metadata(path, value):
            if Path(path).name == 'checkpoint.json':
                raise OSError('fixture checkpoint publication failure')
            return original_write_json(path, value)

        with patch.object(self.saver, 'stamp', return_value='2026-10-19T08:00:00Z'), \
                patch.object(self.saver, 'write_json', side_effect=fail_checkpoint_metadata), \
                patch.object(self.saver, 'write_records', wraps=self.saver.write_records) as write_records:
            with self.assertRaisesRegex(OSError, 'fixture checkpoint publication failure'):
                self.save('checkpoint-failure')
            write_records.assert_not_called()
        self.assertEqual(self.database_state(), before)
        self.assertEqual(self.job(packet['public_id']), job_before)
        self.assertEqual({path: self.hash(path) for path in self.checkpoints()}, checkpoints_before)
        self.assert_shared_checkpoint(self.receipt(first))
        with patch.object(self.saver, 'stamp', return_value='2026-10-19T08:00:00Z'):
            self.save('checkpoint-failure')
        self.assert_saved_once(packet, review)
        self.assertEqual(len(self.checkpoints()), 2)

    def test_corrupt_latest_checkpoint_rejects_save_before_record_mutation(self):
        first, _, _ = self.prepare_submission('checkpoint-good')
        with patch.object(self.saver, 'stamp', return_value='2026-10-18T08:00:00Z'):
            self.save('checkpoint-good')
        backup = Path(self.receipt(first)['backup_path'])
        # Appending bytes preserves a readable SQLite fixture but invalidates
        # its recorded immutable checksum.
        with backup.open('ab') as output:
            output.write(b'changed-checkpoint-bytes')
        corrupted = backup.read_bytes()
        packet, _, _ = self.prepare_submission('checkpoint-corrupt')
        before, job_before = self.database_state(), self.job(packet['public_id'])
        with patch.object(self.saver, 'stamp', return_value='2026-10-18T09:00:00Z'), \
                patch.object(self.saver, 'write_records', wraps=self.saver.write_records) as write_records:
            with self.assertRaisesRegex(ValueError, '(?i)(checkpoint|hash|checksum)'):
                self.save('checkpoint-corrupt')
            write_records.assert_not_called()
        self.assertEqual(self.database_state(), before)
        self.assertEqual(self.job(packet['public_id']), job_before)
        self.assertEqual(self.checkpoints(), [backup])
        self.assertEqual(backup.read_bytes(), corrupted)

    def test_wal_database_checkpoint_is_self_contained_and_includes_committed_wal_rows(self):
        source = sqlite3.connect(self.database)
        self.addCleanup(source.close)
        self.assertEqual(source.execute('PRAGMA journal_mode=WAL').fetchone()[0], 'wal')
        source.execute('UPDATE retained_private_notes SET note=? WHERE id=9',
                       ('Committed source WAL record',))
        source.commit()
        # Keep the writer connection open so these committed bytes still have
        # a live source WAL when SQLite creates the checkpoint.
        wal = self.database.with_name(self.database.name + '-wal')
        self.assertTrue(wal.is_file())
        self.assertGreater(wal.stat().st_size, 0)
        packet, review, _ = self.prepare_submission('wal-checkpoint')
        self.save('wal-checkpoint')
        receipt = self.receipt(packet)
        self.assert_shared_checkpoint(receipt)
        backup = Path(receipt['backup_path'])
        with sqlite3.connect(backup.as_uri() + '?mode=ro&immutable=1', uri=True) as snapshot:
            self.assertEqual(snapshot.execute('PRAGMA integrity_check').fetchone()[0], 'ok')
            self.assertEqual(snapshot.execute('PRAGMA foreign_key_check').fetchall(), [])
            self.assertEqual(snapshot.execute('SELECT note FROM retained_private_notes WHERE id=9').fetchone()[0],
                             'Committed source WAL record')
            self.assertEqual(snapshot.execute('SELECT count(*) FROM ingestion_runs').fetchone()[0], 0)
        for suffix in ('-wal', '-shm', '-journal'):
            self.assertFalse(backup.with_name(backup.name + suffix).exists())
        self.assert_saved_once(packet, review)

    def test_foreign_key_invalid_new_checkpoint_is_not_published_or_pruned(self):
        first, _, _ = self.prepare_submission('checkpoint-before-invalid-fk')
        with patch.object(self.saver, 'stamp', return_value='2026-10-18T08:00:00Z'):
            self.save('checkpoint-before-invalid-fk')
        packet, _, _ = self.prepare_submission('checkpoint-invalid-fk')
        checkpoints_before = {path: self.hash(path) for path in self.checkpoints()}
        # This isolated fixture simulates preexisting damage. The saver must
        # reject its new snapshot before scientific writes or retention.
        with sqlite3.connect(self.database) as db:
            db.execute('PRAGMA foreign_keys=OFF')
            db.execute('INSERT INTO material_evidence(material_id,source_id,claim_scope,claim_json) '
                       'VALUES(?,?,?,?)', (999999, 1, 'fixture.corrupt.foreign_key', '{}'))
            self.assertTrue(db.execute('PRAGMA foreign_key_check').fetchall())
        before, job_before = self.database_state(), self.job(packet['public_id'])
        with patch.object(self.saver, 'stamp', return_value='2026-10-19T08:00:00Z'), \
                patch.object(self.saver, 'write_records', wraps=self.saver.write_records) as write_records:
            with self.assertRaisesRegex(ValueError, '(?i)foreign.?key'):
                self.save('checkpoint-invalid-fk')
            write_records.assert_not_called()
        self.assertEqual(self.database_state(), before)
        self.assertEqual(self.job(packet['public_id']), job_before)
        self.assertEqual(job_before['status'], 'submitted')
        self.assertEqual({path: self.hash(path) for path in self.checkpoints()}, checkpoints_before)
        self.assert_shared_checkpoint(self.receipt(first))

    def test_scientific_review_confirmation_is_required_before_any_mutation(self):
        packet, _, _ = self.prepare_submission('unconfirmed-session')
        before, job_before = self.database_state(), self.job(packet['public_id'])
        with self.assertRaisesRegex(ValueError, '(?i)(confirm|review)'):
            self.saver.save_worker(self.queue, 'unconfirmed-session')
        self.assertEqual(self.database_state(), before)
        self.assertEqual(self.job(packet['public_id']), job_before)


if __name__ == '__main__':
    unittest.main()
