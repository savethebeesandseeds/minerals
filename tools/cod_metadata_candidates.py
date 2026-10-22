"""Map preserved COD JSON metadata to private, unreviewed source candidates.

This module reads no registry database and performs no writes or requests.
``source_value`` and ``source_sigma_value`` retain parsed JSON values and types;
they are not CIF tokens. The archive hash identifies the JSON response, never a
CIF file. Scientific identity and suitability for publication remain unreviewed.
"""

from copy import deepcopy
import hashlib
import json
import math
from pathlib import Path, PurePosixPath
import re


FORMAT = 'waajacu-cod-metadata-candidate-v1'
PILOT = Path('pilots/cod-crystallography-v1')
INDEX_NAME = 'metadata-discovery-execution-index.json'
_SHA = re.compile(r'sha256:[0-9a-f]{64}\Z')
_NUMBER = re.compile(
    r'(?P<sign>[+-]?)(?P<mantissa>(?:\d+(?:\.\d*)?|\.\d+))'
    r'(?:\((?P<uncertainty>\d+)\))?(?P<exponent>[eE][+-]?\d+)?\Z', re.ASCII)
_PLACEHOLDERS = {'', '?', '.', 'null', 'none', 'nan', 'inf', 'infinity',
                 '+inf', '-inf', '+infinity', '-infinity'}
_CELL = ('a', 'b', 'c', 'alpha', 'beta', 'gamma')
_BIBLIOGRAPHY = ('authors', 'title', 'journal', 'year', 'volume', 'issue',
                 'firstpage', 'lastpage', 'doi')


class CandidateError(ValueError):
    """An archive binding or candidate cannot be represented faithfully."""


def _json_copy(value):
    def validate(item):
        if item is None or type(item) in (str, bool, int):
            return
        if type(item) is float and math.isfinite(item):
            return
        if type(item) is list:
            for child in item:
                validate(child)
            return
        if type(item) is dict and all(type(key) is str for key in item):
            for child in item.values():
                validate(child)
            return
        raise CandidateError('Expected finite JSON-compatible data')

    try:
        # Reject non-JSON values and nonfinite Python numeric values as well as
        # preserving independence from subsequent caller mutation.
        validate(value)
        json.dumps(value, allow_nan=False)
    except (TypeError, ValueError) as error:
        raise CandidateError('Expected finite JSON-compatible data') from error
    return deepcopy(value)


def _cod_id(value):
    if isinstance(value, str) and re.fullmatch(r'[0-9]{7}', value):
        return value
    if type(value) is int and 1_000_000 <= value <= 9_999_999:
        return str(value)
    raise CandidateError('COD file must be a seven-digit identifier')


def _revision(value):
    if type(value) is int:
        number = value
    elif isinstance(value, str) and re.fullmatch(r'[1-9][0-9]*', value):
        number = int(value)
    else:
        raise CandidateError('COD revision must be a positive canonical integer')
    if not 0 < number <= 2 ** 64 - 1:
        raise CandidateError('COD revision exceeds the positive unsigned 64-bit range')
    return str(number)


def _present(value):
    return value is not None and not (
        isinstance(value, str) and value.strip().casefold() in _PLACEHOLDERS)


def _number(value):
    """Return a finite number and an optional uncertainty explicitly in text."""
    if type(value) in (int, float):
        try:
            parsed = float(value)
        except OverflowError:
            return None, None
        return (parsed, None) if math.isfinite(parsed) else (None, None)
    if not isinstance(value, str):
        return None, None
    match = _NUMBER.fullmatch(value.strip())
    if not match:
        return None, None
    exponent = match['exponent'] or ''
    try:
        parsed = float(match['sign'] + match['mantissa'] + exponent)
        if parsed == 0 and any(digit in '123456789' for digit in match['mantissa']):
            return None, None  # Do not silently turn underflow into measured zero.
        uncertainty = None
        if match['uncertainty'] is not None:
            decimals = len(match['mantissa'].partition('.')[2])
            power = int(exponent[1:]) if exponent else 0
            uncertainty = int(match['uncertainty']) * 10.0 ** (power - decimals)
        if not math.isfinite(parsed) or (uncertainty is not None and not math.isfinite(uncertainty)):
            return None, None
    except (ValueError, OverflowError):
        return None, None
    return parsed, uncertainty


def _measurement(row, field, sigma, unit):
    original = row.get(field)
    sigma_original = row.get(sigma) if sigma else None
    value, inline_uncertainty = _number(original)
    sigma_value, _ = _number(sigma_original)
    sigma_reported = sigma is not None and _present(sigma_original)
    usable_sigma = sigma_value is not None and sigma_value >= 0
    state = 'numeric' if value is not None else 'missing' if not _present(original) else 'unparseable'
    result = {
        'source_field': field,
        'source_field_present': field in row,
        'source_value': deepcopy(original),
        'source_sigma_field': sigma,
        'source_sigma_field_present': sigma in row if sigma else False,
        'source_sigma_value': deepcopy(sigma_original),
        'state': state,
        'value': value,
        'standard_uncertainty': sigma_value if usable_sigma else inline_uncertainty,
        'uncertainty_source': 'separate_sigma_field' if usable_sigma else
            'value_parentheses' if inline_uncertainty is not None else 'not_reported',
        'unit': unit,
    }
    if sigma_reported and not usable_sigma:
        result['uncertainty_state'] = 'unparseable_or_negative_sigma'
    elif sigma_reported and value is None:
        result['uncertainty_state'] = 'sigma_without_numeric_value'
    elif usable_sigma and inline_uncertainty is not None and sigma_value != inline_uncertainty:
        result['uncertainty_state'] = 'conflicting_explicit_uncertainties'
        result['inline_standard_uncertainty'] = inline_uncertainty
    return result


def map_row(row, locator):
    """Purely map one COD metadata object; never establish mineral identity.

    ``locator`` is a JSON object, normally supplied by ``load_sources``. Direct
    callers must establish its archive binding separately. Unknown source fields
    remain in ``raw_row``. Numeric states describe parsing, not scientific
    validation. Missing angles, conditions and uncertainties are never supplied.
    """
    if not isinstance(row, dict) or not isinstance(locator, dict):
        raise CandidateError('Metadata row and locator must be JSON objects')
    raw = _json_copy(row)
    location = _json_copy(locator)
    cod_id, revision = _cod_id(raw.get('file')), _revision(raw.get('svnrevision'))
    cell = {field: _measurement(raw, field, 'sig' + field,
                               'angstrom' if field in ('a', 'b', 'c') else 'degree')
            for field in _CELL}
    conditions = {
        'cell_temperature': _measurement(raw, 'celltemp', 'sigcelltemp', 'kelvin'),
        'diffraction_temperature': _measurement(raw, 'diffrtemp', 'sigdiffrtemp', 'kelvin'),
        'cell_pressure': _measurement(raw, 'cellpressure', 'sigcellpressure', 'kilopascal'),
        'diffraction_pressure': _measurement(raw, 'diffrpressure', 'sigdiffrpressure', 'kilopascal'),
        'thermal_history': deepcopy(raw.get('thermalhist')),
        'pressure_history': deepcopy(raw.get('pressurehist')),
    }
    origin_value = raw.get('compoundsource')
    text = origin_value if isinstance(origin_value, str) else ''
    markers = [label for label, pattern in (
        ('synthetic_text', r'synth|laboratory|lab[- ]grown'),
        ('theoretical_text', r'theor|ab[- ]?initio|first[- ]principles|density functional|\bDFT\b'),
        ('natural_text', r'\bnatural(?:ly)?\b'),
    ) if re.search(pattern, text, re.IGNORECASE)]
    flags = ['identity_not_validated', 'metadata_candidate_requires_review']
    if not all(item['state'] == 'numeric' for item in cell.values()):
        flags.append('incomplete_or_unparseable_cell')
    if any(item['state'] == 'numeric' and not (item['value'] > 0 if field in ('a', 'b', 'c')
               else 0 < item['value'] < 180) for field, item in cell.items()):
        flags.append('cell_parameter_outside_basic_bounds')
    if not any(_present(raw.get(field)) for field in ('sg', 'sgHall', 'sgNumber')):
        flags.append('space_group_not_reported')
    for field, flag in (('duplicateof', 'cod_duplicate_reference_present'),
                        ('status', 'cod_status_reported'), ('onhold', 'cod_hold_marker_present'),
                        ('flags', 'cod_flags_reported')):
        if _present(raw.get(field)):
            flags.append(flag)
    flags.extend('origin_' + marker for marker in markers)
    if not markers:
        flags.append('origin_text_unclassified' if _present(origin_value) else 'origin_not_reported')
    mapped = {
        'unit_cell': cell,
        'space_group': {field: deepcopy(raw.get(field)) for field in ('sg', 'sgHall', 'sgNumber')},
        'formulas': {field: deepcopy(raw.get(field)) for field in ('formula', 'calcformula', 'cellformula')},
        'formula_units_Z': _measurement(raw, 'Z', None, None),
        'volume': _measurement(raw, 'vol', 'sigvol', 'cubic_angstrom'),
        'conditions': conditions,
        'method': {field: deepcopy(raw.get(field)) for field in
                   ('method', 'radiation', 'wavelength', 'radType', 'radSymbol')},
    }
    z_value = mapped['formula_units_Z']['value']
    if z_value is not None and (z_value <= 0 or not z_value.is_integer()):
        flags.append('formula_units_Z_not_positive_integer')
    if mapped['volume']['value'] is not None and mapped['volume']['value'] <= 0:
        flags.append('reported_volume_not_positive')
    if any(conditions[field]['value'] is not None and conditions[field]['value'] <= 0
           for field in ('cell_temperature', 'diffraction_temperature')):
        flags.append('reported_temperature_not_positive_kelvin')
    measurements = [*cell.values(), mapped['formula_units_Z'], mapped['volume'],
                    *(item for item in conditions.values() if isinstance(item, dict))]
    if any(item.get('uncertainty_state') for item in measurements):
        flags.append('uncertainty_requires_review')
    return {
        'format': FORMAT, 'schema_version': 1, 'cod_id': cod_id, 'revision': revision,
        'review_status': 'unreviewed', 'needs_review': True, 'data_basis': 'saved_cod_metadata',
        'metadata_locator': location, 'raw_row': raw,
        'source': {'record_url': f'https://www.crystallography.net/cod/{cod_id}.html',
                   'pinned_cif_url': f'https://www.crystallography.net/cod/{cod_id}.cif@{revision}',
                   'bibliography': {field: deepcopy(raw.get(field)) for field in _BIBLIOGRAPHY}},
        'mapped': mapped, 'origin': {'raw_value': deepcopy(origin_value), 'markers': markers},
        'review_flags': flags,
    }


def _strict_json(data):
    def unique(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise CandidateError('Duplicate JSON object key')
            result[key] = value
        return result

    def invalid_constant(value):
        raise CandidateError('Nonfinite JSON constant: ' + value)

    def finite_float(value):
        parsed = float(value)
        if not math.isfinite(parsed):
            raise CandidateError('Nonfinite JSON number')
        return parsed

    if data.startswith(b'\xef\xbb\xbf'):
        raise CandidateError('UTF-8 BOM is forbidden in archived JSON')
    try:
        value = json.loads(data.decode('utf-8'), object_pairs_hook=unique,
                           parse_constant=invalid_constant, parse_float=finite_float)
        return value
    except (UnicodeError, ValueError, TypeError) as error:
        raise CandidateError('Invalid archived JSON: ' + str(error)) from error


def _regular_file(data, relative):
    # Archive references use repository-relative POSIX paths beginning data/.
    if not isinstance(relative, str) or '\\' in relative:
        raise CandidateError('Invalid archive path')
    path = PurePosixPath(relative)
    if path.is_absolute() or not path.parts or path.parts[0] != 'data' or '..' in path.parts:
        raise CandidateError('Archive path must stay beneath the supplied data directory')
    target = data
    for part in path.parts[1:]:
        target = target / part
        if target.is_symlink():
            raise CandidateError('Archive paths must not traverse symlinks')
    if not target.is_file():
        raise CandidateError('Missing regular archive file: ' + relative)
    return target


def load_sources(data, wanted):
    """Read exact requested ID/revision pairs from the frozen local archive.

    Each needed response is read and hash-verified once. Conflicting duplicate
    rows, missing exact revisions and invalid archive bindings fail the entire
    call. Repeated identical rows retain their extra locators and original JSON.
    No partial selection, identity promotion, database access or writes occur.
    """
    data = Path(data)
    if not data.is_dir() or data.is_symlink():
        raise CandidateError('Data root must be a real directory')
    requested = set()
    for pair in wanted:
        if not isinstance(pair, (tuple, list)) or len(pair) != 2:
            raise CandidateError('Wanted sources must contain ID/revision pairs')
        requested.add((_cod_id(pair[0]), _revision(pair[1])))
    if not requested:
        return {}
    index_path = _regular_file(data, 'data/' + (PILOT / INDEX_NAME).as_posix())
    index = _strict_json(index_path.read_bytes())
    if (not isinstance(index, dict)
            or index.get('format') != 'waajacu-cod-metadata-discovery-execution-index'
            or index.get('status') != 'complete'):
        raise CandidateError('A completed COD metadata execution index is required')
    snapshot_identity = index.get('snapshot_identity')
    snapshot = snapshot_identity.get('sha256') if isinstance(snapshot_identity, dict) else None
    if not isinstance(snapshot, str) or not _SHA.fullmatch(snapshot):
        raise CandidateError('Missing metadata snapshot identity')
    receipts = {}
    successful_receipts = index.get('successful_receipts')
    if not isinstance(successful_receipts, list):
        raise CandidateError('Missing metadata receipt list')
    for receipt in successful_receipts:
        if not isinstance(receipt, dict):
            raise CandidateError('Invalid metadata receipt')
        prefix = receipt.get('ddd_prefix')
        if not isinstance(prefix, str) or not re.fullmatch(r'[0-9]{3}', prefix) or prefix in receipts:
            raise CandidateError('Invalid or duplicate metadata prefix receipt')
        receipts[prefix] = receipt
    sources = {}
    for prefix in sorted({cod_id[:3] for cod_id, revision in requested}):
        if prefix not in receipts:
            raise CandidateError('No archived response for requested prefix ' + prefix)
        receipt = receipts[prefix]
        response = receipt.get('response')
        if not isinstance(response, dict):
            raise CandidateError('Missing metadata response receipt')
        binding = response.get('raw_body')
        if not isinstance(binding, dict):
            raise CandidateError('Missing metadata response binding')
        digest = binding.get('sha256')
        size = binding.get('bytes')
        if (not isinstance(digest, str) or not _SHA.fullmatch(digest)
                or type(size) is not int or size < 0):
            raise CandidateError('Invalid response hash/size binding')
        bare = digest.removeprefix('sha256:')
        expected = 'data/' + (PILOT / 'objects/sha256' / bare[:2] / bare).as_posix()
        if binding.get('path') != expected:
            raise CandidateError('Response path does not match its content hash')
        if response.get('status_code') != 200:
            raise CandidateError('Archived metadata response was not successful')
        path = _regular_file(data, expected)
        if path.stat().st_size != size:
            raise CandidateError('Archived response byte count changed')
        body = path.read_bytes()
        if len(body) != size or hashlib.sha256(body).hexdigest() != bare:
            raise CandidateError('Archived response hash/size mismatch')
        rows = _strict_json(body)
        if not isinstance(rows, list):
            raise CandidateError('Archived metadata response must be an array')
        seen = {}
        for position, row in enumerate(rows):
            if not isinstance(row, dict):
                raise CandidateError('Archived metadata row must be an object')
            key = _cod_id(row.get('file')), _revision(row.get('svnrevision'))
            if not key[0].startswith(prefix):
                raise CandidateError('COD row does not match its response prefix')
            semantic = {**row, 'file': key[0], 'svnrevision': int(key[1])}
            identity = json.dumps(semantic, sort_keys=True, separators=(',', ':'), allow_nan=False)
            if key in seen and seen[key] != identity:
                raise CandidateError('Conflicting duplicate COD row: ' + '/'.join(key))
            seen[key] = identity
            if key not in requested:
                continue
            locator = {
                'raw_body': deepcopy(binding), 'json_pointer': '/' + str(position),
                'request_ordinal': receipt.get('ordinal'),
                'retrieved_at': response.get('retrieved_at'),
                'response_url': receipt.get('url'), 'snapshot_sha256': snapshot,
            }
            if key in sources:
                sources[key].setdefault('repeated_metadata_rows', []).append(
                    {'metadata_locator': locator, 'raw_row': deepcopy(row)})
            else:
                sources[key] = map_row(row, locator)
    missing = requested - sources.keys()
    if missing:
        examples = ', '.join('/'.join(pair) for pair in sorted(missing)[:5])
        raise CandidateError(f'Missing exact COD revisions ({len(missing)}): {examples}')
    return sources
