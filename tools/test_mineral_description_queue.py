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

    def test_parse_stored_fields_copies_reviewed_claims_only(self):
        queue = self.queue()
        packet = queue.next('writer')
        text = (
            'Beta forms blue crystals, the holotype a clear blue. '
            'It was described from Chile.'
        )
        path = Path(packet['review_file'])
        path.write_text(json.dumps({
            'material_public_id': packet['public_id'],
            'canonical_name': packet['canonical_name'],
            'action': 'compose',
            'text': text,
            'source_urls': ['https://example.test/color'],
            'parse_stored_fields': True,
            'observations': [{'question_key': 'physical.color', 'value': 'invented purple'}],
        }), encoding='utf-8')
        queue.complete('writer', packet['claim_token'], path)
        with sqlite3.connect(self.registry) as database:
            raw = database.execute(
                'SELECT properties_json FROM materials WHERE public_id=?', ('mat_beta',)).fetchone()[0]
        observations = json.loads(raw)['public_research']['observations']
        self.assertEqual(observations, [{
            'question_key': 'physical.color',
            'value': 'blue',
            'review_status': 'reviewed',
            'publication_status': 'published',
            'evidence_level': 'reviewed_catalog_claim',
            'qualification': 'holotype',
        }])
        self.assertEqual(json.loads(raw)['public_research']['cod_records'][0]['raw_cif'], 'SHOULD_NOT_LEAK')

    def test_parsed_fields_keep_nested_measurements_and_skip_private_bundles(self):
        observations = module.field_observations([
            {'review_status': 'reviewed', 'claim_scope': 'physical.color',
             'claim_json': json.dumps({'value': {'colors': ['yellow', 'greenish yellow'],
                                                  'context': 'Skaggs Springs crystalline fraction'}})},
            {'review_status': 'reviewed', 'claim_scope': 'physical.hardness_mohs',
             'claim_json': json.dumps({'value': {'bound': 'less_than', 'unit': 'Mohs', 'value': 2}})},
            {'review_status': 'reviewed', 'claim_scope': 'crystallography.crystal_system',
             'claim_json': json.dumps({'value': {'reported_system': 'orthorhombic',
                                                  'basis': 'powder patterns'}})},
            {'review_status': 'reviewed', 'claim_scope': 'crystallography.unit_cell',
             'claim_json': json.dumps({'value': {'a': '7.474(1)', 'b': '4.980(1)', 'c': '65.85(3)',
                                                  'crystal_system': 'orthorhombic', 'method': 'Powder'}})},
            {'review_status': 'reviewed', 'claim_scope': 'crystallography.unit_cell',
             'claim_json': json.dumps({'value': {'reported_observations': [
                 {'private_research': True, 'a': '1'}]}})},
            {'review_status': 'reviewed', 'claim_scope': 'physical.streak',
             'claim_json': json.dumps({'value': 'red', 'qualification': 'full PDF request returned404'})},
            {'review_status': 'unreviewed', 'claim_scope': 'physical.color',
             'claim_json': json.dumps({'value': 'secret'})},
            {'review_status': 'reviewed', 'claim_scope': 'properties.cod_record_link',
             'claim_json': json.dumps({'value': {'cod_id': '9'}})},
        ])
        by_key = {}
        for item in observations:
            by_key.setdefault(item['question_key'], []).append(item)
        self.assertEqual(by_key['physical.color'][0]['value'], 'yellow, greenish yellow')
        self.assertEqual(by_key['physical.color'][0]['qualification'], 'Skaggs Springs crystalline fraction')
        self.assertEqual(by_key['physical.hardness_mohs'][0]['value'], 'less than 2 Mohs')
        self.assertEqual(by_key['crystallography.crystal_system'][0]['value'], 'Orthorhombic')
        self.assertEqual(by_key['crystallography.crystal_system'][0]['qualification'], 'powder patterns')
        cell = by_key['crystallography.unit_cell'][0]['value']
        self.assertEqual(cell['a'], '7.474(1)')
        self.assertNotIn('crystal_system', cell)
        self.assertNotIn('secret', json.dumps(observations))
        self.assertNotIn('cod_id', json.dumps(observations))
        self.assertEqual(len(by_key['crystallography.unit_cell']), 1)
        self.assertNotIn('qualification', by_key['physical.streak'][0])
        self.assertEqual(by_key['physical.streak'][0]['value'], 'red')
