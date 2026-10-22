#!/usr/bin/env python3
"""Select checked, source-scoped COD metadata for a read-only public export."""

import argparse
from copy import deepcopy
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import sqlite3

from cod_metadata_candidates import load_sources


FORMAT = 'waajacu-cod-public-model-selection-v1'


def require(value, message):
    if not value:
        raise ValueError(message)


def encoded(value):
    return (json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(',', ':'),
                       allow_nan=False) + '\n').encode()


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def present(value):
    return value is not None and value != '' and value != {} and value != [] and value not in ('?', '.')


def quantity(item):
    if not isinstance(item, dict) or not present(item.get('source_value')):
        return None
    result = {'raw_value': item['source_value'], 'source_field': item['source_field']}
    if item.get('state') == 'numeric':
        result['value'] = item['value']
    else:
        result['state'] = item['state']
    for key in ('unit', 'standard_uncertainty', 'uncertainty_state', 'inline_standard_uncertainty'):
        if item.get(key) is not None:
            result[key] = item[key]
    if item.get('source_sigma_value') is not None:
        result['raw_uncertainty'] = item['source_sigma_value']
        result['uncertainty_source_field'] = item['source_sigma_field']
    return result


ASSOCIATION_FIELDS = (
    'cod_id', 'revision', 'status', 'relationship', 'primary_assignment', 'review_status',
    'qualification', 'qualifiers', 'scope', 'sample_origin', 'evidence_level',
    'pinned_structure_url', 'record_url', 'duplicate_of', 'experiment_group',
    'independent_experiment', 'assigned_material_public_id')
ASSOCIATION_FLAGS = ('multiple_saved_associations', 'differing_link_metadata')


def public_associations(links):
    """Publish classifications only when their saved interpretation is public."""
    associations = []
    for link in links:
        if (link.get('publication_status') not in (None, 'published') or
                link.get('review_status') in ('private', 'draft', 'rejected') or
                link.get('visibility') in ('private', 'internal', 'draft') or
                any(link.get(k) is True for k in ('private', 'private_research', 'draft', 'is_draft'))):
            continue
        association = {k: deepcopy(v) for k, v in link.items() if k in ASSOCIATION_FIELDS}
        # No arbitrary supporting-reference or review objects enter this mapping.
        for value in association.values():
            values = value if isinstance(value, list) else [value]
            require(all(isinstance(v, (str, int, float, bool, type(None))) for v in values),
                    'Association qualification must be a public scalar or scalar list')
            require(not any(isinstance(v, str) and re.search(
                r'(?i)(?:/app/|/workspace/|data/pilots/|data/backups/|file:|(?<![a-z])[a-z]:[\\/]|localhost|127\.0\.0\.1)', v)
                for v in values), 'Private path in association qualification')
        associations.append(association)
    return associations


def apply_associations(model, links):
    associations = public_associations(links)
    model['source_associations'] = associations
    for key in ('status', 'relationship'):
        model.pop(key, None)
        values = {a.get(key) for a in associations}
        if len(values) == 1 and next(iter(values)):
            model[key] = next(iter(values))
    if not associations:
        model['status'] = 'metadata_candidate'
        model['relationship'] = 'source_metadata_only'
        model['qualification'] = 'Deposited metadata candidate; no public species interpretation selected.'
    flags = [f for f in model.get('source_quality_flags', []) if f not in ASSOCIATION_FLAGS]
    if len(associations) > 1:
        flags.append('multiple_saved_associations')
        if len({encoded(a) for a in associations}) > 1:
            flags.append('differing_link_metadata')
    if flags:
        model['source_quality_flags'] = flags
    else:
        model.pop('source_quality_flags', None)
    cautions = set(model.get('source_cautions', []))
    for association in associations:
        if (association.get('status') == 'name_conflict' and
                association.get('assigned_material_public_id')):
            cautions.add('This name-conflict source is assigned to another mineral record; '
                         'its association with this mineral remains unresolved.')
        origin = association.get('sample_origin')
        if not present(model.get('sample_origin')) and origin and origin != 'unreported':
            cautions.add('The saved source classification reports specimen origin as ' +
                         str(origin) + '; the structured COD origin field is empty.')
    if cautions:
        model['source_cautions'] = sorted(cautions)
    return model


def public_model(source):
    candidate = source['candidate']
    require(candidate['review_status'] == 'unreviewed' and candidate['needs_review'] is True,
            'Candidate scientific state changed')
    identifier, revision = candidate['cod_id'], candidate['revision']
    require(re.fullmatch(r'[0-9]{7}', identifier) and re.fullmatch(r'[1-9][0-9]*', revision),
            'Invalid COD ID/revision')
    mapped, raw = candidate['mapped'], candidate['raw_row']
    result = {'cod_id': identifier, 'revision': revision, 'review_status': 'unreviewed',
              'needs_review': True, 'primary_assignment': False,
              'data_basis': 'saved_cod_metadata', 'validation_state': 'metadata_extraction_checked',
              'source_url': f'https://www.crystallography.net/cod/{identifier}.cif@{revision}',
              'source_record_url': f'https://www.crystallography.net/cod/{identifier}.html',
              'source_title': raw.get('title') or f'COD {identifier}, revision {revision}',
              'publisher': 'Crystallography Open Database', 'license_spdx': 'CC0-1.0',
              'license_url': 'https://creativecommons.org/publicdomain/zero/1.0/',
              'metadata_source_locator': '#' + candidate['metadata_locator']['json_pointer'],
              'metadata_response_sha256': candidate['metadata_locator']['raw_body']['sha256']}
    result['cell'] = {k: q for k, v in mapped['unit_cell'].items() if (q := quantity(v)) is not None}
    result['symmetry'] = {k: v for k, v in mapped['space_group'].items() if present(v)}
    for target, source_key in (('volume', 'volume'), ('formula_units_Z', 'formula_units_Z')):
        if (value := quantity(mapped[source_key])) is not None:
            result[target] = value
    for target, source_key in (('source_formula', 'formula'), ('source_calculated_formula', 'calcformula'),
                               ('source_unit_cell_formula', 'cellformula')):
        if present(mapped['formulas'].get(source_key)):
            result[target] = mapped['formulas'][source_key]
    conditions = {}
    for key, item in mapped['conditions'].items():
        value = quantity(item) if isinstance(item, dict) else item
        if present(value):
            conditions[key] = value
    if conditions:
        result['conditions'] = conditions
    method = {k: v for k, v in mapped['method'].items() if present(v)}
    if method:
        result['method'] = method
    if present(candidate['origin']['raw_value']):
        result['sample_origin'] = candidate['origin']['raw_value']
    for key, field in (('source_mineral_name', 'mineral'), ('source_sample_description', 'chemname'),
                       ('source_common_name', 'commonname'), ('source_duplicate_of', 'duplicateof'),
                       ('source_status', 'status'), ('source_hold_marker', 'onhold'), ('source_flags', 'flags')):
        if present(raw.get(field)):
            result[key] = raw[field]
    result['bibliography'] = {k: v for k, v in candidate['source']['bibliography'].items() if present(v)}
    result['source_quality_flags'] = [flag for flag in candidate['review_flags'] if flag not in (
        'metadata_candidate_requires_review', 'identity_not_validated', 'cod_flags_reported')]
    if not result['source_quality_flags']:
        result.pop('source_quality_flags')
    result['qualification'] = 'Deposited source model; relationship to this mineral retains the source classifications.'
    result['attribution'] = {
        'attribution_party': raw.get('authors') or 'Crystallography Open Database contributors',
        'work_title': result['source_title'], 'work_url': result['source_url'],
        'license_url': result['license_url'],
        'changes_notice': 'COD metadata formatted as a source-scoped model; original values and qualifications retained.',
        'no_endorsement_notice': 'The source authors and COD do not endorse this atlas.',
        'derived_output_license_spdx': 'CC0-1.0'}
    return apply_associations(result, source['associations'])


def checked_file(path, work, digest=None, max_bytes=32 * 1024 * 1024):
    path = Path(path)
    require(path.is_absolute() and path.resolve().is_relative_to(work.resolve()) and
            path.is_file() and not path.is_symlink(), 'Artifact outside extraction queue or not a regular file')
    require(path.stat().st_size <= max_bytes, 'Extraction artifact exceeds size limit')
    raw = path.read_bytes()
    require(digest is None or sha(raw) == digest, 'Immutable extraction artifact hash mismatch')
    value = json.loads(raw)
    encoded(value)
    return value


def validate_frozen_snapshot(jobs, preparation):
    require(sha(encoded([json.loads(job['record_json']) for job in jobs])) ==
            preparation['record_snapshot_sha256'], 'Frozen record snapshot digest mismatch')


def prepare(root=Path('/workspace'), data=Path('/app/data')):
    root, data = Path(root), Path(data)
    work = data / 'pilots/cod-metadata-extraction-v1'
    connection = sqlite3.connect((work / 'queue.sqlite3').resolve().as_uri() + '?mode=ro', uri=True)
    connection.row_factory = sqlite3.Row
    try:
        connection.execute('BEGIN')
        preparation = json.loads(connection.execute("SELECT value FROM metadata WHERE key='preparation'").fetchone()[0])
        jobs = [dict(r) for r in connection.execute('SELECT * FROM jobs ORDER BY ordinal')]
        validate_frozen_snapshot(jobs, preparation)
        require(jobs and all(r['status'] == 'staged' for r in jobs), 'Finish all extraction assignments first')
        require(len(jobs) == preparation['mineral_count'], 'Prepared queue population changed')
        manifest = json.loads((root / 'public-catalog/catalog-manifest.json').read_bytes())
        require(manifest['release_id'] == preparation['catalog_release_id'],
                'Public base release changed; reconcile this selection explicitly')
        stored = {(r['cod_id'], r['revision']): json.loads(r['candidate_json'])
                  for r in connection.execute('SELECT * FROM sources')}
    finally:
        connection.close()
    verified = load_sources(data, set(stored))
    require(verified == stored, 'Source candidates no longer reproduce the archived metadata')
    qualification_path = work / 'public-qualification-audit-2026-10-22.json'
    qualification_bytes = qualification_path.read_bytes()
    qualification_audit = checked_file(qualification_path, work)
    exclusions, cautions = {}, {}
    for kind, target in (('exclusions', exclusions), ('qualifications', cautions)):
        for item in qualification_audit[kind]:
            key = (item['material_public_id'], item['cod_id'], item['revision'])
            require(isinstance(item['reason'], str) and item['reason'].strip(), 'Missing publication qualification')
            target.setdefault(key, []).append(item)
    records, bindings, deferred, withheld = [], [], [], []
    statistics = {'minerals_reviewed': len(jobs), 'checked_sources': 0, 'deferred_sources': 0,
                  'existing_models_retained': 0, 'new_source_models': 0, 'known_source_issues_withheld': 0,
                  'models_with_source_cautions': 0}
    for job in jobs:
        packet = checked_file(job['packet_path'], work, job['packet_sha256'])
        review = checked_file(job['review_path'], work, job['review_sha256'])
        receipt = checked_file(Path(job['review_path']).with_name('receipt.json'), work)
        require(packet['public_id'] == review['material_public_id'] == receipt['public_id'] == job['public_id'] and
                packet['claim_token'] == review['claim_token'] == receipt['claim_token'] == job['token'] and
                review['packet_sha256'] == receipt['packet_sha256'] == job['packet_sha256'] and
                receipt['review_sha256'] == job['review_sha256'], 'Staging review binding mismatch')
        checks = {(str(c['cod_id']), str(c['revision'])): c for c in review['source_checks']}
        expected = {(str(s['link']['cod_id']), str(s['link']['revision'])) for s in packet['sources']}
        require(len(checks) == len(review['source_checks']) and set(checks) == expected,
                'Review does not cover every source revision once: ' + job['canonical_name'])
        prior = packet['record_context']['material']['properties_json'].get('public_research', {}).get('structures', [])
        prior_pairs = {(str(m.get('cod_id')), str(m.get('revision'))) for m in prior}
        statistics['existing_models_retained'] += len(prior)
        models = []
        for source in packet['sources']:
            candidate = source['candidate']
            key = (candidate['cod_id'], candidate['revision'])
            require(candidate == stored[key], 'Packet source differs from verified metadata')
            check = checks[key]
            require(check['disposition'] in ('checked', 'deferred') and check['note'].strip(), 'Invalid source check')
            if check['disposition'] == 'deferred':
                statistics['deferred_sources'] += 1
                deferred.append({'material_public_id': job['public_id'], 'cod_id': key[0], 'revision': key[1],
                                 'reason': check['note']})
                continue
            statistics['checked_sources'] += 1
            if key in prior_pairs:
                continue
            audit_key = (job['public_id'], key[0], key[1])
            for item in [*exclusions.get(audit_key, []), *cautions.get(audit_key, [])]:
                require(item['source_hash_bindings'] == {'packet_sha256': job['packet_sha256'],
                                                        'review_sha256': job['review_sha256']},
                        'Source qualification audit refers to a different extraction review')
            if audit_key in exclusions:
                statistics['known_source_issues_withheld'] += 1
                withheld.append({'material_public_id': job['public_id'], 'cod_id': key[0], 'revision': key[1],
                                 'reasons': [item['reason'] for item in exclusions[audit_key]]})
                continue
            model = public_model(source)
            if audit_key in cautions:
                model['source_cautions'] = sorted(set(model.get('source_cautions', [])) |
                                                 {item['reason'] for item in cautions[audit_key]})
            if model.get('source_cautions'):
                statistics['models_with_source_cautions'] += 1
            models.append(model)
        if models:
            records.append({'material_public_id': job['public_id'], 'canonical_name': job['canonical_name'],
                            'structures': models})
            statistics['new_source_models'] += len(models)
        bindings.append({'material_public_id': job['public_id'], 'packet_sha256': job['packet_sha256'],
                         'review_sha256': job['review_sha256']})
    selection = {'format': FORMAT, 'schema_version': 1, 'records': records}
    selection_bytes = encoded(selection)
    require(len(selection_bytes) <= 64 * 1024 * 1024, 'Selection exceeds exporter limit')
    spec = importlib.util.spec_from_file_location('cod_public_queue_artifacts', root / 'tools/cod-extraction-queue.py')
    helpers = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(helpers)
    folder = work / 'public-selections' / sha(selection_bytes)
    helpers.immutable_file(folder / 'selection.json', selection_bytes)
    audit = {'format': 'waajacu-cod-public-selection-audit-v1', 'base_release_id': manifest['release_id'],
             'selection_sha256': sha(selection_bytes), 'selection_bytes': len(selection_bytes),
             'source_candidates_sha256': preparation['source_candidates_sha256'],
             'statistics': statistics, 'minerals_with_additions': len(records),
             'qualification_audit_sha256': sha(qualification_bytes),
             'bindings': bindings, 'deferred': deferred, 'withheld': withheld,
             'scope': 'Public source models only; original registry and extraction artifacts unchanged.'}
    helpers.immutable_file(folder / 'audit.json', encoded(audit))
    return {'selection_path': str(folder / 'selection.json'), 'audit_path': str(folder / 'audit.json'),
            'selection_sha256': sha(selection_bytes), 'selection_bytes': len(selection_bytes),
            'statistics': statistics, 'minerals_with_additions': len(records)}


def refine(selection_path, root=Path('/workspace'), data=Path('/app/data')):
    """Reuse a fully verified selection; reread frozen queue context, not packets."""
    root, data = Path(root), Path(data)
    work = data / 'pilots/cod-metadata-extraction-v1'
    selection_path = Path(selection_path)
    prior_audit_path = selection_path.with_name('audit.json')
    prior_audit = checked_file(prior_audit_path, work)
    require(prior_audit.get('format') == 'waajacu-cod-public-selection-audit-v1',
            'Unsupported prior selection audit')
    digest = prior_audit['selection_sha256']
    require(selection_path.name == 'selection.json' and selection_path.parent.name == digest,
            'Prior selection must be content addressed')
    selection = checked_file(selection_path, work, digest, max_bytes=64 * 1024 * 1024)
    require(selection_path.stat().st_size == prior_audit['selection_bytes'] and
            selection.get('format') == FORMAT and selection.get('schema_version') == 1,
            'Prior selection audit does not match selection')
    connection = sqlite3.connect((work / 'queue.sqlite3').resolve().as_uri() + '?mode=ro', uri=True)
    connection.row_factory = sqlite3.Row
    try:
        preparation = json.loads(connection.execute(
            "SELECT value FROM metadata WHERE key='preparation'").fetchone()[0])
        ordered_jobs = [dict(r) for r in connection.execute('SELECT * FROM jobs ORDER BY ordinal')]
        validate_frozen_snapshot(ordered_jobs, preparation)
        jobs = {r['public_id']: r for r in ordered_jobs}
    finally:
        connection.close()
    require(prior_audit['source_candidates_sha256'] == preparation['source_candidates_sha256'] and
            prior_audit['base_release_id'] == preparation['catalog_release_id'],
            'Prior selection preparation binding changed')
    require(jobs and all(r['status'] == 'staged' for r in jobs.values()), 'Queue staging changed')
    bindings = prior_audit['bindings']
    require(len(bindings) == len(jobs) and len({b['material_public_id'] for b in bindings}) == len(jobs),
            'Prior audit does not bind every frozen mineral once')
    for binding in bindings:
        job = jobs.get(binding['material_public_id'])
        require(job is not None and binding['packet_sha256'] == job['packet_sha256'] and
                binding['review_sha256'] == job['review_sha256'], 'Frozen extraction binding changed')
    manifest = json.loads((root / 'public-catalog/catalog-manifest.json').read_bytes())
    require(manifest['release_id'] == prior_audit['base_release_id'], 'Public base release changed')
    records_seen = set()
    for record in selection['records']:
        identifier = record['material_public_id']
        require(identifier in jobs and identifier not in records_seen, 'Unknown or duplicate selected mineral')
        records_seen.add(identifier)
        job = jobs[identifier]
        require(record['canonical_name'] == job['canonical_name'], 'Selected mineral name changed')
        context = json.loads(job['record_json'])
        grouped = {}
        for link in context['properties_json']['cod_records']:
            grouped.setdefault((str(link['cod_id']), str(link['revision'])), []).append(link)
        pairs_seen = set()
        for model in record['structures']:
            key = (str(model['cod_id']), str(model['revision']))
            require(key in grouped and key not in pairs_seen, 'Unknown or duplicate selected source revision')
            pairs_seen.add(key)
            apply_associations(model, grouped[key])
    selection_bytes = encoded(selection)
    require(len(selection_bytes) <= 64 * 1024 * 1024, 'Selection exceeds exporter limit')
    if sha(selection_bytes) == digest:
        return {'selection_path': str(selection_path), 'audit_path': str(prior_audit_path),
                'selection_sha256': digest, 'selection_bytes': len(selection_bytes),
                'statistics': prior_audit['statistics'], 'minerals_with_additions': len(selection['records'])}
    spec = importlib.util.spec_from_file_location('cod_public_queue_artifacts', root / 'tools/cod-extraction-queue.py')
    helpers = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(helpers)
    folder = work / 'public-selections' / sha(selection_bytes)
    helpers.immutable_file(folder / 'selection.json', selection_bytes)
    audit = deepcopy(prior_audit)
    audit.update(selection_sha256=sha(selection_bytes), selection_bytes=len(selection_bytes))
    audit['statistics']['models_with_source_cautions'] = sum(
        bool(model.get('source_cautions')) for record in selection['records']
        for model in record['structures'])
    audit['association_refinement'] = {
        'parent_selection_sha256': digest,
        'parent_audit_sha256': sha(prior_audit_path.read_bytes()),
        'scope': 'Public association qualifications rebuilt from hash-bound frozen queue context; '
                 'all other selected source data and existing cautions retained.'}
    helpers.immutable_file(folder / 'audit.json', encoded(audit))
    return {'selection_path': str(folder / 'selection.json'), 'audit_path': str(folder / 'audit.json'),
            'selection_sha256': sha(selection_bytes), 'selection_bytes': len(selection_bytes),
            'statistics': audit['statistics'], 'minerals_with_additions': len(selection['records'])}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--refine', type=Path, metavar='EXISTING_SELECTION')
    args = parser.parse_args()
    require(Path('/.dockerenv').exists(), 'Run through tools/container-task.sh in the managed admin container')
    print(json.dumps(refine(args.refine) if args.refine else prepare(), ensure_ascii=False, indent=2))


if __name__ == '__main__':
    main()
