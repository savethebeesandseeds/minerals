"""Reservation and description-save tests. They use a temporary database only."""

import importlib.util
import json
from pathlib import Path
import sqlite3
import tempfile
import unittest


spec = importlib.util.spec_from_file_location(
    'mineral_description_queue', Path(__file__).with_name('mineral-description-queue.py'))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
DescriptionQueue = module.DescriptionQueue


class DescriptionQueueTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='minerals-description-queue-test-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.data = self.root / 'private-data'
        self.data.mkdir()
        self.registry = self.data / 'minerals.db'
        long_text = (
            'Alpha is a silicate mineral with formula SiO2. The reviewed record already '
            'summarizes its identity, and this text is long enough to keep.'
        )
        properties = {
            'alpha': {'public_research': {
                'publication_status': 'published', 'review_status': 'reviewed',
                'description': {'text': long_text, 'source_urls': ['https://example.test/alpha']},
                'cod_records': [{'cod_id': '1', 'raw_cif': 'SHOULD_NOT_LEAK'}],
            }},
            'beta': {'public_research': {
                'publication_status': 'published', 'review_status': 'reviewed',
                'cod_records': [{'cod_id': '2', 'raw_cif': 'SHOULD_NOT_LEAK'}],
            }},
            'gamma': {'public_research': {
                'publication_status': 'published', 'review_status': 'reviewed',
                'description': {'text': 'Short.', 'source_urls': ['https://example.test/gamma']},
            }},
        }
        with sqlite3.connect(self.registry) as database:
            database.executescript('''
                CREATE TABLE materials(
                    id INTEGER PRIMARY KEY, public_id TEXT UNIQUE, canonical_name TEXT,
                    formula TEXT, description TEXT, record_type TEXT, properties_json TEXT,
                    nomenclature_status TEXT, is_valid_species INTEGER, search_text TEXT,
                    updated_at TEXT);
                CREATE TABLE mineral_dataset_facts(
                    material_id INTEGER, fact_key TEXT, fact_value TEXT);
                CREATE TABLE material_evidence(
                    id INTEGER PRIMARY KEY, material_id INTEGER, claim_scope TEXT,
                    claim_json TEXT, review_status TEXT, source_work_url TEXT);
            ''')
            rows = [
                (1, 'mat_alpha', 'Alpha', 'SiO2', '', 'mineral', json.dumps(properties['alpha']),
                 'approved', 1, 'alpha', ''),
                (2, 'mat_beta', 'Beta', 'SiO2', '', 'mineral', json.dumps(properties['beta']),
                 'questionable', 1, 'beta', ''),
                (3, 'mat_gamma', 'Gamma', 'SiO2', '', 'mineral', json.dumps(properties['gamma']),
                 'approved', 1, 'gamma', ''),
            ]
            database.executemany('INSERT INTO materials VALUES(?,?,?,?,?,?,?,?,?,?,?)', rows)
            database.execute(
                'INSERT INTO mineral_dataset_facts VALUES(?,?,?)',
                (2, 'discovery_country', 'Chile'))
            database.execute(
                'INSERT INTO mineral_dataset_facts VALUES(?,?,?)',
                (2, 'first_reference', 'Beta 1901'))
            database.executemany(
                'INSERT INTO material_evidence VALUES(?,?,?,?,?,?)', [
                    (1, 2, 'physical.color', json.dumps({'value': 'blue', 'qualification': 'holotype'}),
                     'reviewed', 'https://example.test/color'),
                    (2, 2, 'physical.color', json.dumps({'value': 'secret-unreviewed'}),
                     'unreviewed', None),
                    (3, 2, 'properties.cod_record_link', json.dumps({'value': {'cod_id': '2'}}),
                     'reviewed', None),
                ])
        self.long_text = long_text

    def queue(self):
        queue = DescriptionQueue(self.root, self.data, lease_seconds=60)
        self.addCleanup(queue.close)
        return queue

    def review(self, packet, text, urls, action=None):
        path = Path(packet['review_file'])
        path.write_text(json.dumps({
            'material_public_id': packet['public_id'],
            'canonical_name': packet['canonical_name'],
            'action': action or packet['known']['action'],
            'text': text,
            'source_urls': urls,
        }), encoding='utf-8')
        return path

    def test_missing_records_are_claimed_before_existing_descriptions(self):
        queue = self.queue()
        first = queue.next('worker-a')
        second = queue.next('worker-b')
        self.assertEqual(first['public_id'], 'mat_beta')
        self.assertEqual(first['known']['action'], 'compose')
        self.assertEqual(first['known']['discovery_country'], 'Chile')
        self.assertEqual(first['known']['reviewed_claims'][0]['value'], 'blue')
        self.assertNotIn('secret-unreviewed', json.dumps(first))
        self.assertNotIn('SHOULD_NOT_LEAK', json.dumps(first))
        self.assertEqual(first['known']['cod_summary']['public_cod_records'], 1)
        self.assertEqual(second['public_id'], 'mat_gamma')
        self.assertEqual(second['priority_group'], 'thin')
        status = queue.status()
        self.assertEqual(status['priority_groups']['missing']['in_progress'], 1)
        self.assertEqual(status['priority_groups']['retain']['pending'], 1)
        resumed = queue.next('worker-a')
        self.assertEqual(resumed['claim_token'], first['claim_token'])
        self.assertFalse(resumed['new_assignment'])

    def test_compose_writes_the_record_and_keeps_cod_records(self):
        queue = self.queue()
        packet = queue.next('writer')
        text = (
            'Beta forms blue crystals, the holotype a clear blue. '
            'It was described from Chile.'
        )
        path = self.review(packet, text, ['https://example.test/color'])
        result = queue.complete('writer', packet['claim_token'], path)
        self.assertEqual(result['action'], 'compose')
        with sqlite3.connect(self.registry) as database:
            database.row_factory = sqlite3.Row
            saved = database.execute('SELECT * FROM materials WHERE public_id=?', ('mat_beta',)).fetchone()
        self.assertEqual(saved['description'], text)
        properties = json.loads(saved['properties_json'])
        description = properties['public_research']['description']
        self.assertEqual(description['text'], text)
        self.assertEqual(description['license_spdx'], 'CC-BY-4.0')
        self.assertEqual(description['production_kind'], 'record_summary')
        self.assertEqual(properties['public_research']['cod_records'][0]['raw_cif'], 'SHOULD_NOT_LEAK')
        self.assertIn(text, saved['search_text'])
        self.assertEqual(queue.status()['counts']['complete'], 1)

    def test_compose_rejects_a_new_source_url(self):
        queue = self.queue()
        packet = queue.next('writer')
        text = 'Beta forms blue crystals and was described from Chile. ' + ('The crystals are sharp. ' * 2)
        path = self.review(packet, text, ['https://example.test/new'])
        with self.assertRaises(ValueError):
            queue.complete('writer', packet['claim_token'], path)

    def test_retain_rejects_a_rewrite(self):
        queue = self.queue()
        queue.next('other')
        queue.next('other-thin')
        packet = queue.next('keeper')
        self.assertEqual(packet['public_id'], 'mat_alpha')
        path = self.review(packet, self.long_text + ' Extra.', ['https://example.test/alpha'])
        with self.assertRaises(ValueError):
            queue.complete('keeper', packet['claim_token'], path)
        path = self.review(packet, self.long_text, ['https://example.test/alpha'])
        queue.complete('keeper', packet['claim_token'], path)
        with sqlite3.connect(self.registry) as database:
            description = database.execute(
                'SELECT description FROM materials WHERE public_id=?', ('mat_alpha',)).fetchone()[0]
        self.assertEqual(description, self.long_text)

    def test_thin_description_is_replaced_and_the_old_text_is_kept(self):
        queue = self.queue()
        queue.next('skip-missing')
        packet = queue.next('rewriter')
        self.assertEqual(packet['priority_group'], 'thin')
        text = (
            'Gamma forms small crystals. The earlier note only said that the mineral was short, '
            'and this header keeps to that stored record.'
        )
        path = self.review(packet, text, ['https://example.test/gamma'])
        queue.complete('rewriter', packet['claim_token'], path)
        with sqlite3.connect(self.registry) as database:
            raw = database.execute(
                'SELECT properties_json FROM materials WHERE public_id=?', ('mat_gamma',)).fetchone()[0]
        properties = json.loads(raw)
        self.assertEqual(properties['public_research']['description']['text'], text)
        self.assertEqual(properties['research_draft_descriptions'][0]['text'], 'Short.')
        self.assertEqual(properties['research_draft_descriptions'][0]['publication_status'], 'private')
