"""Fixture tests for the public, source-scoped COD metadata selection."""

import copy
import importlib.util
import json
from pathlib import Path
import sys
import sqlite3
import tempfile
import unittest


sys.path.insert(0, str(Path(__file__).parent))
from cod_metadata_candidates import map_row

SPEC = importlib.util.spec_from_file_location(
    'prepare_cod_public_selection', Path(__file__).with_name('prepare-cod-public-selection.py'))
selection = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(selection)


class PublicModelSelectionTests(unittest.TestCase):
    def setUp(self):
        self.row = {
            'file': '9017418', 'svnrevision': 283658,
            'a': '5.123(4)', 'siga': '0.0200', 'b': '5.12300', 'c': '13.48(2)',
            'alpha': 90, 'beta': '90', 'gamma': '120',
            'sg': 'P 1', 'sgHall': '-P 1', 'sgNumber': '159',
            'formula': '- C2 H Na O7 Pb2 -', 'calcformula': '- C2 H Na O7 Pb2 -',
            'cellformula': '- C4 H2 Na2 O14 Pb4 -', 'Z': '2', 'vol': '323.975',
            'compoundsource': 'Synthetic crystal from hydrothermal synthesis',
            'mineral': 'Source mineral name', 'chemname': 'Reported compound',
            'commonname': 'Source common name',
            'method': 'single crystal', 'radiation': 'X-ray', 'wavelength': '0.71073',
            'radType': 'Mo Kα', 'radSymbol': 'Mo',
            'title': 'Synthesis and crystal structure of a carbonate',
            'authors': 'A. Researcher; B. Author', 'year': '2020',
            'journal': 'Example crystallography journal', 'volume': '12',
            'firstpage': '101', 'lastpage': '109', 'doi': '10.1234/example.2020',
            'celltemp': None, 'diffrtemp': '?', 'cellpressure': '.',
            'unknown_private_field': {'path': '/app/data/private/transport.json'},
        }
        self.locator = {
            'raw_body': {'path': 'data/pilots/private/objects/' + 'a' * 64,
                         'sha256': 'sha256:' + 'a' * 64, 'bytes': 1234},
            'json_pointer': '/7418', 'request_ordinal': 901,
            'retrieved_at': '2026-10-05T14:43:20.162Z',
            'response_url': 'https://www.crystallography.net/cod/result?format=json&id=901%25',
            'snapshot_sha256': 'sha256:' + 'b' * 64,
        }
        self.associations = [
            {'cod_id': '9017418', 'revision': '283658', 'status': 'name_conflict',
             'relationship': 'synthetic_counterpart', 'primary_assignment': True,
             'review_status': 'reviewed', 'qualification': 'Synthetic counterpart; identity disputed',
             'qualifiers': ['Source name differs'], 'scope': 'deposited_model'},
            {'cod_id': '9017418', 'revision': '283658', 'status': 'candidate',
             'relationship': 'historical_model', 'primary_assignment': False,
             'review_status': 'unreviewed', 'qualification': 'Historical candidate retained',
             'scope': 'deposited_model'},
        ]

    def source(self):
        return {'candidate': map_row(self.row, self.locator),
                'associations': copy.deepcopy(self.associations)}

    def test_quantity_preserves_raw_precision_and_conflicting_uncertainties(self):
        original = self.source()['candidate']['mapped']['unit_cell']['a']
        before = copy.deepcopy(original)
        result = selection.quantity(original)
        self.assertEqual(result['raw_value'], '5.123(4)')
        self.assertEqual(result['value'], 5.123)
        self.assertEqual(result['raw_uncertainty'], '0.0200')
        self.assertEqual(result['standard_uncertainty'], 0.02)
        self.assertEqual(result['inline_standard_uncertainty'], 0.004)
        self.assertEqual(result['uncertainty_state'], 'conflicting_explicit_uncertainties')
        self.assertEqual(result['source_field'], 'a')
        self.assertEqual(result['uncertainty_source_field'], 'siga')
        self.assertEqual(result['unit'], 'angstrom')
        self.assertEqual(original, before)

    def test_quantity_omits_missing_values_but_keeps_zero_and_unparseable_reports(self):
        for absent in (None, {}, {'source_value': None}, {'source_value': ''},
                       {'source_value': '?'}, {'source_value': '.'}):
            with self.subTest(absent=absent):
                self.assertIsNone(selection.quantity(absent))
        self.row.update(celltemp=0, cellpressure='not numeric')
        conditions = self.source()['candidate']['mapped']['conditions']
        self.assertEqual(selection.quantity(conditions['cell_temperature'])['value'], 0)
        result = selection.quantity(conditions['cell_pressure'])
        self.assertEqual(result['raw_value'], 'not numeric')
        self.assertEqual(result['state'], conditions['cell_pressure']['state'])
        self.assertNotIn('value', result)

    def test_public_scientific_status_remains_unreviewed_and_nonprimary(self):
        source = self.source()
        before = copy.deepcopy(source)
        result = selection.public_model(source)
        self.assertEqual(result['review_status'], 'unreviewed')
        self.assertIs(result['needs_review'], True)
        self.assertIs(result['primary_assignment'], False)
        self.assertEqual(result['data_basis'], 'saved_cod_metadata')
        self.assertEqual(result['validation_state'], 'metadata_extraction_checked')
        self.assertNotIn('status', result)
        self.assertNotIn('relationship', result)
        self.assertEqual(source, before)
        for key, value in (('review_status', 'reviewed'), ('needs_review', False)):
            with self.subTest(key=key):
                invalid = copy.deepcopy(source)
                invalid['candidate'][key] = value
                with self.assertRaisesRegex(ValueError, 'scientific state changed'):
                    selection.public_model(invalid)

    def test_all_associations_keep_their_distinct_status_scope_and_qualifications(self):
        source = self.source()
        source['associations'][0]['private_artifact_path'] = '/app/data/private/review.json'
        result = selection.public_model(source)
        self.assertEqual(result['source_associations'], self.associations)
        self.assertEqual(len(result['source_associations']), 2)
        result['source_associations'][0]['qualifiers'].append('new local display note')
        self.assertEqual(source['associations'][0]['qualifiers'], ['Source name differs'])
        single = self.source()
        single['associations'] = [single['associations'][1]]
        reduced = selection.public_model(single)
        self.assertEqual(reduced['status'], 'candidate')
        self.assertEqual(reduced['relationship'], 'historical_model')
        self.assertIs(reduced['primary_assignment'], False)

    def test_missing_conditions_stay_absent_and_symmetry_does_not_infer_species_facts(self):
        result = selection.public_model(self.source())
        self.assertNotIn('conditions', result)
        self.assertEqual(result['symmetry'], {'sg': 'P 1', 'sgHall': '-P 1', 'sgNumber': '159'})
        for field in ('crystal_system', 'density', 'species_identity', 'mineral_id'):
            self.assertNotIn(field, result)
            self.assertNotIn(field, result['symmetry'])
        self.assertEqual(result['source_formula'], self.row['formula'])
        self.assertIn('uncertainty_requires_review', result['source_quality_flags'])

    def test_association_qualifications_preserve_types_and_visible_conflicts(self):
        source = self.source()
        link = source['associations'][0]
        link.update(sample_origin='synthetic', evidence_level='saved_metadata',
                    pinned_structure_url='https://www.crystallography.net/cod/9017418.cif@283658',
                    record_url='https://www.crystallography.net/cod/9017418.html',
                    duplicate_of='9017417', experiment_group='published-experiment-1',
                    independent_experiment=False, assigned_material_public_id='mat_other')
        link['revision'] = 283658
        source['associations'] = [link]
        source['candidate']['origin']['raw_value'] = None
        result = selection.public_model(source)
        self.assertEqual(result['source_associations'][0], link)
        self.assertIsInstance(result['source_associations'][0]['revision'], int)
        self.assertIs(result['source_associations'][0]['independent_experiment'], False)
        self.assertTrue(any('another mineral' in s for s in result['source_cautions']))
        self.assertTrue(any('synthetic' in s for s in result['source_cautions']))

    def test_private_interpretations_do_not_publish_labels_or_qualifications(self):
        source = self.source()
        private = dict(source['associations'][0], publication_status='private',
                       status='identified', relationship='identity_supported',
                       qualifiers=['Secret scientific interpretation'])
        source['associations'] = [private]
        result = selection.public_model(source)
        self.assertEqual(result['source_associations'], [])
        self.assertEqual(result['status'], 'metadata_candidate')
        self.assertEqual(result['relationship'], 'source_metadata_only')
        self.assertIs(result['primary_assignment'], False)
        self.assertNotIn('Secret scientific interpretation', json.dumps(result))
        self.assertEqual(result['cell']['a']['raw_value'], '5.123(4)')
        source['associations'].append(self.associations[1])
        result = selection.public_model(source)
        self.assertEqual(result['source_associations'], [self.associations[1]])
        self.assertEqual(result['relationship'], 'historical_model')

    def test_refinement_reuses_verified_selection_without_packet_reads(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder) / 'root'
            work = Path(folder) / 'data/pilots/cod-metadata-extraction-v1'
            work.mkdir(parents=True)
            (root / 'public-catalog').mkdir(parents=True)
            (root / 'public-catalog/catalog-manifest.json').write_bytes(
                selection.encoded({'release_id': 'fixture-release'}))
            (root / 'tools').mkdir()
            (root / 'tools/cod-extraction-queue.py').write_text(
                'def immutable_file(path, raw):\n'
                '    path.parent.mkdir(parents=True, exist_ok=True)\n'
                '    if path.exists():\n'
                '        assert path.read_bytes() == raw\n'
                '    else:\n'
                '        path.write_bytes(raw)\n')
            model = selection.public_model(self.source())
            model['source_cautions'] = ['Existing reviewed source caution']
            payload = {'format': selection.FORMAT, 'schema_version': 1, 'records': [
                {'material_public_id': 'mat_fixture', 'canonical_name': 'Fixture', 'structures': [model]}]}
            raw = selection.encoded(payload)
            parent = work / 'public-selections' / selection.sha(raw)
            parent.mkdir(parents=True)
            path = parent / 'selection.json'
            path.write_bytes(raw)
            binding = {'material_public_id': 'mat_fixture', 'packet_sha256': 'a' * 64,
                       'review_sha256': 'b' * 64}
            audit = {'format': 'waajacu-cod-public-selection-audit-v1',
                     'base_release_id': 'fixture-release', 'selection_sha256': selection.sha(raw),
                     'source_candidates_sha256': 'c' * 64,
                     'selection_bytes': len(raw), 'bindings': [binding],
                     'statistics': {'models_with_source_cautions': 1}, 'withheld': [], 'deferred': []}
            audit_raw = selection.encoded(audit)
            (parent / 'audit.json').write_bytes(audit_raw)
            links = [dict(self.associations[0], publication_status='private',
                          qualifiers=['Private unpublished qualification']),
                     dict(self.associations[1], sample_origin='synthetic', evidence_level='saved_metadata')]
            database = sqlite3.connect(work / 'queue.sqlite3')
            database.execute('CREATE TABLE jobs(public_id,canonical_name,status,record_json,packet_sha256,review_sha256,ordinal)')
            frozen_record = {'properties_json': {'cod_records': links}}
            database.execute('INSERT INTO jobs VALUES(?,?,?,?,?,?,?)', (
                'mat_fixture', 'Fixture', 'staged', json.dumps({'properties_json': {'cod_records': links}}),
                binding['packet_sha256'], binding['review_sha256'], 0))
            database.execute('CREATE TABLE metadata(key,value)')
            database.execute('INSERT INTO metadata VALUES(?,?)', ('preparation', json.dumps({
                'record_snapshot_sha256': selection.sha(selection.encoded([frozen_record])),
                'source_candidates_sha256': 'c' * 64, 'catalog_release_id': 'fixture-release'})))
            database.commit()
            database.close()
            result = selection.refine(path, root, Path(folder) / 'data')
            self.assertNotEqual(Path(result['selection_path']), path)
            refined = json.loads(Path(result['selection_path']).read_bytes())
            revised = refined['records'][0]['structures'][0]
            self.assertEqual(revised['source_associations'], [links[1]])
            self.assertEqual(revised['status'], 'candidate')
            self.assertEqual(revised['cell'], model['cell'])
            self.assertIn('Existing reviewed source caution', revised['source_cautions'])
            self.assertNotIn('Private unpublished qualification', json.dumps(refined))
            self.assertEqual(path.read_bytes(), raw)
            self.assertEqual((parent / 'audit.json').read_bytes(), audit_raw)
            updated_audit = json.loads(Path(result['audit_path']).read_bytes())
            self.assertEqual(updated_audit['bindings'], audit['bindings'])
            self.assertEqual(updated_audit['association_refinement']['parent_audit_sha256'],
                             selection.sha(audit_raw))
            self.assertEqual(selection.refine(path, root, Path(folder) / 'data'), result)
            self.assertEqual(selection.refine(Path(result['selection_path']), root,
                                              Path(folder) / 'data'), result)
            # Corrupt either the selection or its queue binding: refinement must stop.
            path.write_bytes(raw + b' ')
            with self.assertRaisesRegex(ValueError, 'hash mismatch'):
                selection.refine(path, root, Path(folder) / 'data')
            path.write_bytes(raw)
            database = sqlite3.connect(work / 'queue.sqlite3')
            database.execute("UPDATE jobs SET record_json='{}'")
            database.commit()
            database.close()
            with self.assertRaisesRegex(ValueError, 'snapshot digest mismatch'):
                selection.refine(path, root, Path(folder) / 'data')
            database = sqlite3.connect(work / 'queue.sqlite3')
            database.execute('UPDATE jobs SET record_json=?', (json.dumps(frozen_record),))
            database.execute("UPDATE jobs SET packet_sha256='changed'")
            database.commit()
            database.close()
            with self.assertRaisesRegex(ValueError, 'binding changed'):
                selection.refine(path, root, Path(folder) / 'data')

    def test_only_reported_conditions_are_selected_without_conversion_or_defaults(self):
        self.row.update(celltemp='0', diffrtemp='100(2)', cellpressure='101.325',
                        diffrpressure='2500000', thermalhist='annealed at 500 K',
                        pressurehist='decompressed after synthesis')
        result = selection.public_model(self.source())
        conditions = result['conditions']
        self.assertEqual(conditions['cell_temperature']['value'], 0)
        self.assertEqual(conditions['cell_temperature']['raw_value'], '0')
        self.assertEqual(conditions['cell_temperature']['unit'], 'kelvin')
        self.assertEqual(conditions['diffraction_temperature']['raw_value'], '100(2)')
        self.assertEqual(conditions['diffraction_temperature']['standard_uncertainty'], 2)
        self.assertEqual(conditions['cell_pressure']['value'], 101.325)
        self.assertEqual(conditions['cell_pressure']['unit'], 'kilopascal')
        self.assertEqual(conditions['diffraction_pressure']['value'], 2500000)
        self.assertEqual(conditions['thermal_history'], 'annealed at 500 K')
        self.assertEqual(conditions['pressure_history'], 'decompressed after synthesis')

    def test_public_provenance_excludes_transport_paths_and_private_reviews(self):
        source = self.source()
        source.update(review_path='/app/data/private/review.json',
                      review_note='Private review prose', claim_token='private-claim-token')
        source['candidate']['private_review_notes'] = ['Private review prose']
        result = selection.public_model(source)
        self.assertEqual(result['metadata_source_locator'], '#/7418')
        self.assertEqual(result['metadata_response_sha256'], 'sha256:' + 'a' * 64)
        self.assertEqual(result['source_url'], 'https://www.crystallography.net/cod/9017418.cif@283658')
        self.assertEqual(result['source_record_url'], 'https://www.crystallography.net/cod/9017418.html')
        for field in ('raw_row', 'metadata_locator', 'raw_body', 'request_ordinal',
                      'response_url', 'snapshot_sha256', 'review_path', 'review_note',
                      'private_review_notes', 'claim_token', 'unknown_private_field'):
            self.assertNotIn(field, result)
        text = json.dumps(result)
        for private in ('/app/data/private', 'data/pilots/private', 'private-claim-token',
                        'Private review prose', 'sha256:' + 'b' * 64):
            self.assertNotIn(private, text)

    def test_origin_formulas_method_bibliography_and_full_credits_are_retained(self):
        source = self.source()
        result = selection.public_model(source)
        self.assertEqual(result['sample_origin'], self.row['compoundsource'])
        self.assertEqual(result['source_mineral_name'], self.row['mineral'])
        self.assertEqual(result['source_sample_description'], self.row['chemname'])
        self.assertEqual(result['source_common_name'], self.row['commonname'])
        self.assertEqual(result['source_calculated_formula'], self.row['calcformula'])
        self.assertEqual(result['source_unit_cell_formula'], self.row['cellformula'])
        self.assertEqual(result['method'], source['candidate']['mapped']['method'])
        self.assertEqual(result['bibliography'], {
            k: v for k, v in source['candidate']['source']['bibliography'].items()
            if selection.present(v)})
        self.assertEqual(result['bibliography']['authors'], self.row['authors'])
        self.assertEqual(result['bibliography']['doi'], self.row['doi'])
        self.assertEqual(result['publisher'], 'Crystallography Open Database')
        self.assertEqual(result['license_spdx'], 'CC0-1.0')
        self.assertEqual(result['license_url'], 'https://creativecommons.org/publicdomain/zero/1.0/')
        credits = result['attribution']
        self.assertEqual(credits['attribution_party'], self.row['authors'])
        self.assertEqual(credits['work_title'], self.row['title'])
        self.assertEqual(credits['work_url'], result['source_url'])
        self.assertEqual(credits['license_url'], result['license_url'])
        self.assertEqual(credits['derived_output_license_spdx'], 'CC0-1.0')
        self.assertTrue(credits['changes_notice'])
        self.assertTrue(credits['no_endorsement_notice'])


if __name__ == '__main__':
    unittest.main()
