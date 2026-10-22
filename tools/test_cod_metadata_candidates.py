"""Check faithful COD metadata mapping and archive binding in isolated fixtures."""

import copy
import hashlib
import json
from pathlib import Path
import sys
import tempfile
import unittest


sys.path.insert(0, str(Path(__file__).parent))
from cod_metadata_candidates import CandidateError, load_sources, map_row


class MetadataMappingTests(unittest.TestCase):
    def setUp(self):
        self.row = {
            'file': '9017418', 'svnrevision': 283658,
            'a': '5.268', 'b': 5.268, 'c': '13.48(2)',
            'alpha': 90, 'beta': '90', 'gamma': '120',
            'sg': 'P 3 1 c', 'sgHall': 'P 3 -2c', 'sgNumber': 159,
            'formula': '- C2 H Na O7 Pb2 -', 'calcformula': '- C2 H Na O7 Pb2 -',
            'Z': '2', 'vol': '323.975', 'compoundsource': 'Synthetic',
            'celltemp': None, 'diffrtemp': '?', 'cellpressure': '.',
            'title': 'Synthesis and crystal structure of a carbonate',
            'unknown_future_field': {'list': [None, False, 7, ' raw text ']},
        }
        self.locator = {
            'raw_body': {'path': 'data/pilots/cod-crystallography-v1/objects/sha256/aa/' + 'a' * 64,
                         'sha256': 'sha256:' + 'a' * 64, 'bytes': 1},
            'json_pointer': '/0', 'request_ordinal': 901,
            'retrieved_at': '2026-10-05T14:43:20.162Z',
            'response_url': 'https://www.crystallography.net/cod/result?format=json&id=901%25',
            'snapshot_sha256': 'sha256:' + 'b' * 64,
        }

    def test_abellaite_source_remains_synthetic_unreviewed_and_source_scoped(self):
        candidate = map_row(self.row, self.locator)
        self.assertEqual(candidate['cod_id'], '9017418')
        self.assertEqual(candidate['revision'], '283658')
        self.assertEqual(candidate['review_status'], 'unreviewed')
        self.assertTrue(candidate['needs_review'])
        self.assertEqual(candidate['data_basis'], 'saved_cod_metadata')
        self.assertEqual(candidate['origin'], {'raw_value': 'Synthetic', 'markers': ['synthetic_text']})
        self.assertIn('identity_not_validated', candidate['review_flags'])
        self.assertEqual(candidate['source']['pinned_cif_url'],
                         'https://www.crystallography.net/cod/9017418.cif@283658')
        self.assertNotIn('material_public_id', candidate)
        self.assertNotIn('primary_assignment', candidate)
        self.assertNotIn('publication_status', candidate)

    def test_original_json_fields_types_and_locator_are_independent_copies(self):
        row_before, locator_before = copy.deepcopy(self.row), copy.deepcopy(self.locator)
        candidate = map_row(self.row, self.locator)
        self.assertEqual(candidate['raw_row'], row_before)
        self.assertEqual(candidate['metadata_locator'], locator_before)
        self.row['unknown_future_field']['list'].append('later mutation')
        self.locator['raw_body']['bytes'] = 999
        self.assertEqual(candidate['raw_row'], row_before)
        self.assertEqual(candidate['metadata_locator'], locator_before)
        self.assertIsInstance(candidate['mapped']['unit_cell']['b']['source_value'], float)
        self.assertIsInstance(candidate['mapped']['unit_cell']['a']['source_value'], str)

    def test_numeric_uncertainty_keeps_source_value_and_explicit_sigma(self):
        self.row.update(a='5.123(4)', siga='0.004')
        candidate = map_row(self.row, self.locator)
        value = candidate['mapped']['unit_cell']['a']
        self.assertEqual(value['source_value'], '5.123(4)')
        self.assertEqual(value['source_sigma_value'], '0.004')
        self.assertEqual(value['value'], 5.123)
        self.assertAlmostEqual(value['standard_uncertainty'], 0.004)
        self.assertEqual(value['source_field'], 'a')
        self.assertEqual(value['source_sigma_field'], 'siga')
        self.assertEqual(value['unit'], 'angstrom')
        self.assertNotIn('uncertainty_requires_review', candidate['review_flags'])

    def test_conflicting_uncertainty_is_retained_and_flagged(self):
        self.row.update(a='5.123(4)', siga='0.02')
        candidate = map_row(self.row, self.locator)
        value = candidate['mapped']['unit_cell']['a']
        self.assertEqual(value['source_value'], '5.123(4)')
        self.assertEqual(value['source_sigma_value'], '0.02')
        self.assertAlmostEqual(value['inline_standard_uncertainty'], 0.004)
        self.assertAlmostEqual(value['standard_uncertainty'], 0.02)
        self.assertEqual(value['uncertainty_state'], 'conflicting_explicit_uncertainties')
        self.assertIn('uncertainty_requires_review', candidate['review_flags'])

    def test_parenthesized_uncertainty_scales_with_exponent(self):
        self.row['a'] = '5.123(4)e1'
        value = map_row(self.row, self.locator)['mapped']['unit_cell']['a']
        self.assertAlmostEqual(value['value'], 51.23)
        self.assertAlmostEqual(value['standard_uncertainty'], 0.04)
        self.assertEqual(value['uncertainty_source'], 'value_parentheses')

    def test_missing_and_unparseable_fields_are_not_repaired_from_symmetry(self):
        for source_value in (None, '', '?', '.', 'NaN', 'Infinity'):
            with self.subTest(source_value=source_value):
                self.row['beta'] = source_value
                candidate = map_row(self.row, self.locator)
                value = candidate['mapped']['unit_cell']['beta']
                self.assertEqual(value['state'], 'missing')
                self.assertIsNone(value['value'])
                self.assertEqual(value['source_value'], source_value)
                self.assertIn('incomplete_or_unparseable_cell', candidate['review_flags'])
        self.row['beta'] = '90 degrees approximately'
        value = map_row(self.row, self.locator)['mapped']['unit_cell']['beta']
        self.assertEqual(value['state'], 'unparseable')
        self.assertIsNone(value['value'])

    def test_absent_field_is_distinguished_from_explicit_json_null(self):
        self.row.pop('a')
        self.row['b'] = None
        cell = map_row(self.row, self.locator)['mapped']['unit_cell']
        self.assertFalse(cell['a']['source_field_present'])
        self.assertTrue(cell['b']['source_field_present'])
        self.assertIsNone(cell['a']['value'])
        self.assertIsNone(cell['b']['value'])

    def test_nonpositive_lengths_and_bad_angles_are_flagged_without_change(self):
        self.row.update(a='0', b='-2', gamma='180')
        candidate = map_row(self.row, self.locator)
        cell = candidate['mapped']['unit_cell']
        self.assertEqual(cell['a']['value'], 0)
        self.assertEqual(cell['b']['value'], -2)
        self.assertEqual(cell['gamma']['value'], 180)
        self.assertIn('cell_parameter_outside_basic_bounds', candidate['review_flags'])

    def test_space_group_forms_and_formula_are_not_reconciled_or_normalized(self):
        self.row.update(sg='P 1', sgHall='-P 1', sgNumber='159', formula='raw formula ?')
        candidate = map_row(self.row, self.locator)
        self.assertEqual(candidate['mapped']['space_group'],
                         {'sg': 'P 1', 'sgHall': '-P 1', 'sgNumber': '159'})
        self.assertEqual(candidate['mapped']['formulas']['formula'], 'raw formula ?')
        self.assertNotIn('crystal_system', candidate['mapped'])
        self.assertNotIn('density', candidate['mapped'])

    def test_missing_conditions_origin_and_volume_do_not_gain_defaults(self):
        for key in ('compoundsource', 'celltemp', 'diffrtemp', 'cellpressure', 'vol'):
            self.row.pop(key, None)
        candidate = map_row(self.row, self.locator)
        for key in ('cell_temperature', 'diffraction_temperature', 'cell_pressure', 'diffraction_pressure'):
            self.assertIsNone(candidate['mapped']['conditions'][key]['value'])
        self.assertIsNone(candidate['mapped']['volume']['value'])
        self.assertEqual(candidate['origin'], {'raw_value': None, 'markers': []})
        self.assertIn('origin_not_reported', candidate['review_flags'])

    def test_conditions_keep_individual_fields_and_cod_units_without_conversion(self):
        self.row.update(celltemp='293(2)', diffrtemp='100',
                        cellpressure='101.325', diffrpressure='2500000')
        conditions = map_row(self.row, self.locator)['mapped']['conditions']
        self.assertEqual(conditions['cell_temperature']['value'], 293)
        self.assertEqual(conditions['cell_temperature']['unit'], 'kelvin')
        self.assertEqual(conditions['diffraction_temperature']['value'], 100)
        self.assertEqual(conditions['diffraction_temperature']['unit'], 'kelvin')
        self.assertEqual(conditions['cell_pressure']['value'], 101.325)
        self.assertEqual(conditions['cell_pressure']['unit'], 'kilopascal')
        self.assertEqual(conditions['diffraction_pressure']['value'], 2500000)
        self.assertEqual(conditions['diffraction_pressure']['unit'], 'kilopascal')
        self.assertEqual(conditions['diffraction_pressure']['source_value'], '2500000')

    def test_conflicting_origin_words_and_cod_status_remain_visible(self):
        self.row.update(compoundsource='Natural and synthetic samples; DFT comparison',
                        status='retracted', onhold='yes', duplicateof='9019999', flags=['errors'])
        candidate = map_row(self.row, self.locator)
        self.assertEqual(set(candidate['origin']['markers']),
                         {'natural_text', 'synthetic_text', 'theoretical_text'})
        for flag in ('cod_status_reported', 'cod_hold_marker_present',
                     'cod_duplicate_reference_present', 'cod_flags_reported'):
            self.assertIn(flag, candidate['review_flags'])
        self.assertEqual(candidate['raw_row']['status'], 'retracted')
        self.assertEqual(candidate['review_status'], 'unreviewed')

    def test_nonfinite_or_non_json_values_are_rejected(self):
        for value in (float('nan'), float('inf'), {1, 2}, (1, 2)):
            with self.subTest(value=repr(value)):
                row = dict(self.row, a=value)
                with self.assertRaises(CandidateError):
                    map_row(row, self.locator)

    def test_id_and_revision_do_not_accept_fuzzy_or_implicit_values(self):
        for field, value in [('file', '9017418.html'), ('file', True),
                             ('svnrevision', None), ('svnrevision', 0),
                             ('svnrevision', '0283658'), ('svnrevision', 'latest')]:
            with self.subTest(field=field, value=value):
                row = dict(self.row)
                row[field] = value
                with self.assertRaises(CandidateError):
                    map_row(row, self.locator)


class ArchiveBindingTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='cod-metadata-archive-test-')
        self.addCleanup(self.temp.cleanup)
        self.data = Path(self.temp.name)
        self.pilot = self.data / 'pilots/cod-crystallography-v1'
        self.index_path = self.pilot / 'metadata-discovery-execution-index.json'
        self.row = {'file': '9017418', 'svnrevision': 283658, 'a': '5.123(4)',
                    'compoundsource': 'Synthetic', 'unknown': {'preserve': ['all fields']}}
        self.index = {
            'format': 'waajacu-cod-metadata-discovery-execution-index',
            'status': 'complete', 'snapshot_identity': {'sha256': 'sha256:' + 'b' * 64},
            'successful_receipts': [],
        }
        self.add_response('901', [self.row])
        self.write_index()

    def write_index(self):
        self.index_path.parent.mkdir(parents=True, exist_ok=True)
        self.index_path.write_text(json.dumps(self.index))

    def add_response(self, prefix, rows=None, raw=None):
        if raw is None:
            raw = json.dumps(rows, ensure_ascii=False).encode('utf-8')
        digest = hashlib.sha256(raw).hexdigest()
        relative = f'data/pilots/cod-crystallography-v1/objects/sha256/{digest[:2]}/{digest}'
        path = self.data.joinpath(*relative.split('/')[1:])
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(raw)
        receipt = {'ddd_prefix': prefix, 'ordinal': int(prefix),
                   'url': f'https://www.crystallography.net/cod/result?id={prefix}%25&format=json',
                   'response': {'status_code': 200, 'retrieved_at': '2026-10-05T14:43:20.162Z',
                                'raw_body': {'path': relative, 'sha256': 'sha256:' + digest,
                                             'bytes': len(raw)}}}
        self.index['successful_receipts'] = [r for r in self.index['successful_receipts']
                                             if r['ddd_prefix'] != prefix] + [receipt]
        return path

    def snapshot(self):
        return {path.relative_to(self.data): path.read_bytes()
                for path in self.data.rglob('*') if path.is_file()}

    def test_hash_bound_exact_row_locator_is_read_without_writes(self):
        before = self.snapshot()
        sources = load_sources(self.data, {('9017418', '283658')})
        self.assertEqual(set(sources), {('9017418', '283658')})
        candidate = sources[('9017418', '283658')]
        self.assertEqual(candidate['raw_row'], self.row)
        self.assertEqual(candidate['metadata_locator']['json_pointer'], '/0')
        self.assertEqual(candidate['metadata_locator']['snapshot_sha256'], 'sha256:' + 'b' * 64)
        self.assertEqual(self.snapshot(), before)

    def test_missing_exact_revision_does_not_substitute_another_revision(self):
        before = self.snapshot()
        with self.assertRaises(CandidateError):
            load_sources(self.data, {('9017418', '1')})
        self.assertEqual(self.snapshot(), before)

    def test_conflicting_duplicate_source_rows_fail_entire_load(self):
        self.add_response('901', [self.row, dict(self.row, a='999')])
        self.write_index()
        with self.assertRaises(CandidateError):
            load_sources(self.data, {('9017418', '283658')})

    def test_identical_duplicates_keep_every_original_locator(self):
        row = copy.deepcopy(self.row)
        row['file'], row['svnrevision'] = 9017418, '283658'
        self.add_response('901', [self.row, row])
        self.write_index()
        candidate = load_sources(self.data, {('9017418', '283658')})[('9017418', '283658')]
        self.assertEqual(candidate['raw_row'], self.row)
        repeated = candidate['repeated_metadata_rows']
        self.assertEqual(repeated[0]['metadata_locator']['json_pointer'], '/1')
        self.assertEqual(repeated[0]['raw_row'], row)

    def test_archive_tampering_and_receipt_path_mismatch_are_rejected(self):
        receipt = self.index['successful_receipts'][0]
        path = self.data.joinpath(*receipt['response']['raw_body']['path'].split('/')[1:])
        original = path.read_bytes()
        path.write_bytes(original.replace(b'Synthetic', b'synthetic'))
        with self.assertRaises(CandidateError):
            load_sources(self.data, {('9017418', '283658')})
        path.write_bytes(original)
        receipt['response']['raw_body']['path'] = 'data/../outside'
        self.write_index()
        with self.assertRaises(CandidateError):
            load_sources(self.data, {('9017418', '283658')})

    def test_archive_symlink_is_rejected_without_following_it(self):
        receipt = self.index['successful_receipts'][0]
        path = self.data.joinpath(*receipt['response']['raw_body']['path'].split('/')[1:])
        target = self.data / 'copy.json'
        target.write_bytes(path.read_bytes())
        path.unlink()
        path.symlink_to(target)
        with self.assertRaises(CandidateError):
            load_sources(self.data, {('9017418', '283658')})

    def test_noncanonical_or_ambiguous_archived_json_is_rejected(self):
        invalid = [b'[{"file":"9017418","file":"9017418","svnrevision":283658}]',
                   b'[{"file":"9017418","svnrevision":283658,"a":NaN}]',
                   b'[{"file":"9017418","svnrevision":283658,"a":1e999}]',
                   b'\xef\xbb\xbf[{"file":"9017418","svnrevision":283658}]',
                   b'{}']
        for raw in invalid:
            with self.subTest(raw=raw):
                self.add_response('901', raw=raw)
                self.write_index()
                with self.assertRaises(CandidateError):
                    load_sources(self.data, {('9017418', '283658')})

    def test_wrong_prefix_or_incomplete_index_fails(self):
        self.add_response('901', [dict(self.row, file='9027418')])
        self.write_index()
        with self.assertRaises(CandidateError):
            load_sources(self.data, {('9017418', '283658')})
        self.add_response('901', [self.row])
        self.index['status'] = 'in_progress'
        self.write_index()
        with self.assertRaises(CandidateError):
            load_sources(self.data, {('9017418', '283658')})

    def test_empty_request_needs_no_archive_access(self):
        with tempfile.TemporaryDirectory(prefix='cod-empty-cache-test-') as root:
            self.assertEqual(load_sources(Path(root), set()), {})


if __name__ == '__main__':
    unittest.main()
