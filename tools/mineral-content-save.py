#!/usr/bin/env python3
"""Save a worker's scientifically reviewed submissions privately and atomically."""

import argparse
import copy
from contextlib import contextmanager
from datetime import datetime, timezone
import fcntl
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import sqlite3
import uuid


CHECKPOINT_INTERVAL_SECONDS = 24 * 60 * 60
CHECKPOINT_RETENTION = 2
CHECKPOINT_DIRECTORY = re.compile(r'checkpoint-[0-9a-f]{32}')
BATCH_CHECKS = {'record_preservation': 'ok', 'evidence_links': 'ok', 'foreign_keys': 'enforced',
                'search_updates': 'ok', 'cod_assignments': 'ok'}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def stamp():
    return datetime.now(timezone.utc).isoformat().replace('+00:00', 'Z')


def encoded(value):
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(',', ':'),
                      allow_nan=False, default=lambda b: {'blob_sha256': hashlib.sha256(b).hexdigest()})


def file_hash(path):
    digest = hashlib.sha256()
    with Path(path).open('rb') as source:
        while chunk := source.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def write_json(path, value):
    """Publish complete, durable metadata; recovery must never read a partial receipt."""
    path = Path(path)
    temporary = path.with_name('.' + path.name + '.' + uuid.uuid4().hex + '.tmp')
    try:
        with temporary.open('x', encoding='utf-8') as output:
            output.write(encoded(value) + '\n')
            output.flush()
            os.fsync(output.fileno())
        os.replace(temporary, path)
        directory = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
        try:
            os.fsync(directory)
        finally:
            os.close(directory)
    finally:
        if temporary.exists():
            temporary.unlink()


def sync_directory(path):
    descriptor = os.open(path, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def checkpoint_directories(queue):
    root = queue.work / 'database-checkpoints'
    require(not root.is_symlink(), 'Checkpoint directory must not be a symlink')
    existed = root.exists()
    root.mkdir(exist_ok=True)
    require(root.resolve().is_relative_to(queue.data.resolve()), 'Checkpoint directory is outside private data')
    if not existed:
        sync_directory(queue.work)
    folders = []
    for path in root.iterdir():
        if CHECKPOINT_DIRECTORY.fullmatch(path.name):
            require(path.is_dir() and not path.is_symlink(), 'Invalid managed checkpoint directory')
            folders.append(path)
    return root, folders


def remove_checkpoint(queue, folder):
    files = list(folder.iterdir())
    allowed = {'checkpoint.json', 'minerals.sqlite3', 'minerals.sqlite3.pending'}
    allowed.update(name + suffix for name in ('minerals.sqlite3', 'minerals.sqlite3.pending')
                   for suffix in ('-journal', '-wal', '-shm'))
    for path in files:
        require(path.name in allowed or re.fullmatch(r'\.checkpoint\.json\.[0-9a-f]{32}\.tmp', path.name),
                'Unexpected file in retired checkpoint: ' + str(path))
        queue.private_file(path)
    # Retire the publication marker first. Interrupted retirement must never
    # leave a published entry pointing at missing snapshot bytes.
    marker = folder / 'checkpoint.json'
    if marker.exists():
        marker.unlink()
        sync_directory(folder)
    for path in files:
        if path != marker:
            path.unlink()
    folder.rmdir()


def prune_checkpoints(queue, keep):
    """Only retire files in this helper's new namespace, after a durable checkpoint exists."""
    root, folders = checkpoint_directories(queue)
    for folder in folders:
        if folder not in keep:
            remove_checkpoint(queue, folder)
    sync_directory(root)


def check_cod_assignments(connection, cod_ids=None):
    """Ordinary reviews do no COD scan; primary-link reviews check affected IDs."""
    if cod_ids is not None and not cod_ids:
        return
    sql = ("WITH primary_links AS ("
           "SELECT material_id,json_extract(claim_json,'$.value.cod_id') AS cod_id FROM material_evidence "
           "WHERE (claim_scope='properties.cod_record_link' OR json_extract(claim_json,'$.question_key')='properties.cod_record_link') "
           "AND json_extract(claim_json,'$.value.primary_assignment')=1 UNION ALL "
           "SELECT e.material_id,json_extract(j.value,'$.value.cod_id') FROM material_evidence e, "
           "json_each(e.claim_json,'$.value.reported_observations') j "
           "WHERE json_extract(e.claim_json,'$.question_key')='properties.cod_record_link' "
           "AND json_extract(j.value,'$.value.primary_assignment')=1) SELECT cod_id FROM primary_links ")
    parameters = sorted(cod_ids) if cod_ids is not None else []
    if parameters:
        sql += 'WHERE CAST(cod_id AS TEXT) IN (' + ','.join('?' for _ in parameters) + ') '
    sql += 'GROUP BY CAST(cod_id AS TEXT) HAVING count(DISTINCT material_id)>1'
    require(not connection.execute(sql, parameters).fetchall(), 'Conflicting primary COD assignments')


def full_database_audit(connection, now):
    """Run expensive global checks once for each new daily checkpoint."""
    require(connection.execute('PRAGMA integrity_check').fetchall() == [('ok',)],
            'Shared checkpoint integrity check failed')
    require(not connection.execute('PRAGMA foreign_key_check').fetchall(),
            'Shared checkpoint foreign key check failed')
    connection.execute("INSERT INTO material_search(material_search,rank) VALUES('integrity-check',1)")
    check_cod_assignments(connection)
    return {'integrity': 'ok', 'foreign_key_violations': 0, 'FTS_integrity': 'ok',
            'conflicting_primary_targets': 0, 'checked_at': now}


def create_checkpoint(queue, root, now):
    folder = root / ('checkpoint-' + uuid.uuid4().hex)
    folder.mkdir()
    sync_directory(root)
    temporary, backup = folder / 'minerals.sqlite3.pending', folder / 'minerals.sqlite3'
    try:
        # Use a separate reader: backup() on the reserved writer connection can hang.
        source = sqlite3.connect((queue.data / 'minerals.db').resolve().as_uri() + '?mode=ro', uri=True)
        try:
            destination = sqlite3.connect(temporary)
            try:
                source.backup(destination)
                audit = full_database_audit(destination, now)
                destination.commit()
            finally:
                destination.close()
        finally:
            source.close()
        with temporary.open('rb') as content:
            os.fsync(content.fileno())
        digest = file_hash(temporary)
        os.replace(temporary, backup)
        sync_directory(folder)
        info = backup.stat()
        metadata = {'version': 1, 'checkpoint': folder.name, 'created_at': now, 'sha256': digest,
                    'bytes': info.st_size, 'mtime_ns': info.st_mtime_ns, 'audit': audit}
        write_json(folder / 'checkpoint.json', metadata)
        return folder, metadata
    except BaseException:
        # Keep a published checkpoint if an error occurred during its directory
        # fsync. Otherwise discard this attempt; the prior checkpoint is intact.
        if not (folder / 'checkpoint.json').exists():
            remove_checkpoint(queue, folder)
            sync_directory(root)
        raise


def shared_checkpoint(queue, now):
    """Reuse one daily SQLite recovery checkpoint across all serialized workers.

    SQLite's transaction journal handles batch rollback. Accepted payloads and
    receipts handle committed-save reconciliation; neither needs a full copy
    for every batch. The newest two periodic checkpoints are kept separately.
    Caller holds save.lock and BEGIN IMMEDIATE, preventing intervening writes.
    """
    root, folders = checkpoint_directories(queue)
    current_time = datetime.fromisoformat(now.replace('Z', '+00:00'))
    checkpoints = []
    for folder in folders:
        marker = folder / 'checkpoint.json'
        if not marker.exists():
            continue  # Interrupted creation/retirement; clean after a valid replacement.
        metadata = json.loads(queue.private_file(marker).read_text(encoding='utf-8'))
        require(metadata.get('version') == 1 and metadata.get('checkpoint') == folder.name and
                re.fullmatch(r'[0-9a-f]{64}', metadata.get('sha256', '')),
                'Invalid checkpoint metadata: ' + str(marker))
        if 'audit' in metadata:
            audit = metadata['audit']
            require(isinstance(audit, dict) and audit.get('integrity') == 'ok' and
                    audit.get('foreign_key_violations') == 0 and audit.get('FTS_integrity') == 'ok' and
                    audit.get('conflicting_primary_targets') == 0 and audit.get('checked_at') == metadata['created_at'],
                    'Invalid checkpoint audit')
        created = datetime.fromisoformat(metadata['created_at'].replace('Z', '+00:00'))
        require(created.tzinfo is not None and created <= current_time, 'Invalid or future checkpoint timestamp')
        checkpoints.append((created, folder, metadata))
    checkpoints.sort(key=lambda entry: (entry[0], entry[1].name), reverse=True)
    if (checkpoints and (current_time - checkpoints[0][0]).total_seconds() < CHECKPOINT_INTERVAL_SECONDS
            and 'audit' in checkpoints[0][2]):
        _, folder, metadata = checkpoints[0]
        backup = queue.private_file(folder / 'minerals.sqlite3')
        info = backup.stat()
        if (info.st_size, info.st_mtime_ns) != (metadata.get('bytes'), metadata.get('mtime_ns')):
            require(file_hash(backup) == metadata['sha256'], 'Shared checkpoint hash mismatch')
            metadata.update(bytes=info.st_size, mtime_ns=info.st_mtime_ns)
            write_json(folder / 'checkpoint.json', metadata)
    else:
        folder, metadata = create_checkpoint(queue, root, now)
        backup = folder / 'minerals.sqlite3'
        checkpoints.insert(0, (current_time, folder, metadata))
    # Prune after checkpoint publication, even if a later batch rolls back. This
    # also bounds storage when saves fail repeatedly across several days.
    prune_checkpoints(queue, {entry[1] for entry in checkpoints[:CHECKPOINT_RETENTION]})
    return {'backup_path': str(backup), 'backup_sha256': metadata['sha256'],
            'backup_policy': 'shared_daily_checkpoint', 'backup_created_at': metadata['created_at'],
            'backup_interval_seconds': CHECKPOINT_INTERVAL_SECONDS,
            'backup_retained_checkpoints': CHECKPOINT_RETENTION,
            'checkpoint_audit': metadata['audit'],
            'backup_scope': 'periodic_recovery_checkpoint; not a per-batch undo image; expires on rotation'}


@contextmanager
def save_lock(queue, wait=True):
    """Serialize every helper instance, while normal research claims remain independent."""
    descriptor = os.open(queue.work / 'save.lock', os.O_CREAT | os.O_RDWR, 0o600)
    try:
        try:
            fcntl.flock(descriptor, fcntl.LOCK_EX | (0 if wait else fcntl.LOCK_NB))
        except BlockingIOError:
            yield False
            return
        yield True
    finally:
        fcntl.flock(descriptor, fcntl.LOCK_UN)
        os.close(descriptor)


def public_state(root):
    catalog = Path(root) / 'public-catalog'
    require((catalog / 'catalog-manifest.json').is_file(), 'Public catalog manifest is missing')
    return {str(p.relative_to(catalog)): file_hash(p) for p in catalog.rglob('*') if p.is_file()}


def validate_sources(queue, record):
    """Verify archived original bytes, retaining access failures as retrieval notes only."""
    own = Path(record.get('review_artifact_directory') or record['submission_directory'])
    own = own.resolve()
    require(own.is_relative_to(queue.data.resolve()) and own.is_dir(), 'Review directory is outside private data')
    for source in record['sources']:
        require(isinstance(source.get('publisher'), str) and source['publisher'].strip(), 'Source publisher is required')
        captures, receipts = [], []

        def capture(path, digest):
            require(isinstance(digest, str) and digest, 'Source capture requires its original SHA256')
            path = Path(path)
            if not path.is_absolute():
                path = own / path
            resolved = queue.private_file(path)
            require(file_hash(resolved) == digest.removeprefix('sha256:'), 'Source capture hash mismatch: ' + str(path))
            with resolved.open('rb') as content:
                magic = content.read(8)
            if resolved.suffix.lower() == '.pdf':
                require(magic.startswith(b'%PDF-'), 'Archived PDF is not genuine PDF bytes')
            if resolved.suffix.lower() == '.doc':
                require(magic == bytes.fromhex('d0cf11e0a1b11ae1'), 'Archived DOC is not genuine document bytes')
            item = {'path': str(resolved), 'sha256': digest.removeprefix('sha256:')}
            if item not in captures:
                captures.append(item)

        if source.get('source_file'):
            capture(source['source_file'], source.get('content_sha256') or source.get('content_hash') or source.get('archive', {}).get('sha256'))
        if source.get('archived_file'):
            capture(source['archived_file'], source.get('content_hash'))
        if source.get('archive'):
            capture(source['archive']['archive_path'], source['archive']['sha256'])
        if source.get('pdf_source_file'):
            capture(source['pdf_source_file'], source.get('pdf_content_sha256'))
        for archive in source.get('additional_archives', []) + source.get('supplementary_archives', []):
            capture(archive.get('source_file') or archive['archive_path'], archive.get('content_sha256') or archive.get('sha256'))
        for key in ('additional_source_capture', 'alternate_archive', 'accepted_manuscript_archive'):
            if source.get(key):
                archive = source[key]
                capture(archive.get('archived_file') or archive['archive_path'], archive.get('content_hash') or archive.get('sha256'))
        for receipt in source.get('retrieval_receipts', []):
            if isinstance(receipt, str):
                path = Path(receipt)
                path = queue.private_file(path if path.is_absolute() else own / path)
                data = json.loads(path.read_text(encoding='utf-8-sig'))
                receipts.append({'path': str(path), 'sha256': file_hash(path), 'status': data.get('status')})
            else:
                require(isinstance(receipt, dict), 'Retrieval receipt must be a path or object')
                data = receipt
            if data.get('content_is_publication'):
                capture(data.get('archive_path') or data.get('path'), data.get('sha256'))
        source['validated_captures'] = captures
        if receipts:
            source['validated_retrieval_receipts'] = receipts


def submitted_records(queue, worker, maximum):
    jobs = [dict(row) for row in queue.db.execute(
        "SELECT * FROM jobs WHERE worker=? AND status='submitted' ORDER BY claimed_at,public_id LIMIT ?",
        (worker, maximum))]
    records = []
    for job in jobs:
        path = queue.private_file(job['submission_path'])
        require(file_hash(path) == job['submission_sha256'], 'Immutable submission hash mismatch')
        record = json.loads(path.read_text(encoding='utf-8-sig'))
        queue.validate_review(record, job)
        for claim in record['claims']:
            require(claim.get('qualification') and claim.get('evidence_level'), 'Every reviewed claim needs qualifications and evidence level')
            require(not {'review_batch', 'reviewed_at'} & claim.keys(), 'New claims must not supply save-owned review metadata')
        draft = record['editorial_draft']
        require(draft['text'].strip(), 'A readable private description draft is required')
        ids = {claim['claim_id'] for claim in record['claims']}
        require(set(draft.get('supporting_claim_ids', [])) <= ids, 'Draft cites an unknown claim')
        record['content_queue_claim'] = {'claim_token': job['token'], 'submission_sha256': job['submission_sha256']}
        record['submission_directory'] = str(path.parent)
        validate_sources(queue, record)
        records.append(record)
    return jobs, records


def recover_worker(queue, worker):
    """A prepared receipt is proof only when its exact ingestion actually committed."""
    jobs = {r['public_id']: dict(r) for r in queue.db.execute(
        "SELECT * FROM jobs WHERE worker=? AND status='submitted'", (worker,))}
    recovered = []
    if not jobs:
        return recovered
    for path in sorted((queue.work / 'saves').glob('*/commit-receipt.json')):
        receipt = json.loads(queue.private_file(path).read_text(encoding='utf-8-sig'))
        matches = []
        for binding in receipt.get('queue_claims', []):
            job = jobs.get(binding['public_id'])
            if job and binding == {'public_id': job['public_id'], 'worker': worker,
                                   'claim_token': job['token'], 'submission_sha256': job['submission_sha256']}:
                matches.append(job['public_id'])
        if not matches:
            continue
        with queue.minerals() as connection:
            run = connection.execute('SELECT source_label,metadata_json,status FROM ingestion_runs WHERE id=?',
                                     (receipt['ingestion_run_id'],)).fetchone()
        if not run or run['source_label'] != receipt['review_batch']:
            continue  # A rolled-back ingestion ID can later be reused by SQLite.
        require(run['status'] == 'completed' and json.loads(run['metadata_json']).get('payload_sha256') == receipt['payload_sha256'],
                'Committed save does not match its recovery receipt')
        for public_id in matches:
            queue.complete(public_id, path)
            recovered.append(public_id)
            del jobs[public_id]
    return recovered


@contextmanager
def batch_write_guard(connection):
    """Reject writes outside the save's narrow table/column contract, including triggers."""
    # Connect FTS before installing the guard: SQLite initializes a virtual
    # table through internal schema authorization events on its first use.
    connection.execute('SELECT rowid FROM material_search LIMIT 0')
    connection.execute("SELECT 1 FROM json_each('[]') LIMIT 0")
    shadows = {row['name'] for row in connection.execute('PRAGMA table_list')
               if row['type'] == 'shadow' and row['name'].startswith('material_search_')}

    def authorize(action, table, column, database, origin):
        if action in (sqlite3.SQLITE_INSERT, sqlite3.SQLITE_UPDATE, sqlite3.SQLITE_DELETE):
            if database != 'main':
                return sqlite3.SQLITE_DENY
            if table in shadows and origin in (None, 'materials_search_update'):
                return sqlite3.SQLITE_OK
            if action == sqlite3.SQLITE_INSERT:
                if origin is None and table in ('evidence_sources', 'material_evidence', 'ingestion_runs'):
                    return sqlite3.SQLITE_OK
                if table == 'material_search' and origin == 'materials_search_update':
                    return sqlite3.SQLITE_OK
            if (action == sqlite3.SQLITE_UPDATE and origin is None and table == 'materials'
                    and column in ('properties_json', 'search_text')):
                return sqlite3.SQLITE_OK
            return sqlite3.SQLITE_DENY
        if action in (sqlite3.SQLITE_READ, sqlite3.SQLITE_SELECT, sqlite3.SQLITE_FUNCTION,
                      sqlite3.SQLITE_TRANSACTION, sqlite3.SQLITE_SAVEPOINT, sqlite3.SQLITE_RECURSIVE):
            return sqlite3.SQLITE_OK
        if action == sqlite3.SQLITE_PRAGMA and table == 'data_version' and column is None:
            return sqlite3.SQLITE_OK  # FTS checks its cache version when updating/searching.
        return sqlite3.SQLITE_DENY

    connection.set_authorizer(authorize)
    try:
        yield
    finally:
        connection.set_authorizer(None)


def row_hash(row):
    return hashlib.sha256(encoded(dict(row)).encode()).hexdigest()


def validate_batch(connection, records, results, batch, run_id, payload_hash, inserted, intended_evidence):
    """Read back this batch's rows and actual FTS matches, without scanning prior history."""
    affected_cod = set()
    require(len(records) == len(results), 'Saved result count mismatch')
    for record, result in zip(records, results):
        require(result['material_public_id'] == record['material_public_id'], 'Saved result identity mismatch')
        material = connection.execute('SELECT * FROM materials WHERE public_id=?',
                                      (record['material_public_id'],)).fetchone()
        require(material and row_hash(material) == result['material_sha256'],
                'Saved material differs from the intended update')
        expected_claims = {claim['claim_id']: claim for claim in record['claims']}
        linked = set()
        for evidence_id in result['evidence_ids']:
            evidence = connection.execute(
                'SELECT e.*,s.canonical_url FROM material_evidence e JOIN evidence_sources s ON s.id=e.source_id '
                'WHERE e.id=?', (evidence_id,)).fetchone()
            require(evidence and evidence['material_id'] == material['id'] and
                    evidence['source_release_id'] == batch and evidence['review_status'] == 'reviewed',
                    'Inserted evidence does not match this batch')
            require(all(evidence[key] == value for key, value in intended_evidence[evidence_id].items()),
                    'Saved evidence values or citation snapshots differ from the intended update')
            bundle = json.loads(evidence['claim_json'])
            require(bundle.get('review_batch') == batch and bundle.get('observation_ids'), 'Invalid evidence bundle')
            for claim_id in bundle['observation_ids']:
                claim = expected_claims.get(claim_id)
                require(claim and claim_id not in linked and claim['source_url'] == evidence['canonical_url'] and
                        claim['question_key'] == bundle.get('question_key'), 'Evidence claim linkage mismatch')
                linked.add(claim_id)
        require(linked == set(expected_claims), 'Saved claims are missing evidence links')
        if record['claims']:
            # Every new observation embeds this fresh UUID in search_text.
            # MATCH checks the actual FTS index rather than external content.
            token = batch.rsplit('-', 1)[1]
            require(connection.execute(
                'SELECT rowid FROM material_search WHERE material_search MATCH ? AND rowid=?',
                ('search_text : "' + token + '"', material['id'])).fetchone(), 'Saved search update is missing')
        for claim in record['claims']:
            value = claim['value']
            if (claim['question_key'] == 'properties.cod_record_link' and isinstance(value, dict)
                    and value.get('primary_assignment') == 1):
                cod_id = value.get('cod_id')
                require(isinstance(cod_id, (str, int)) and not isinstance(cod_id, bool) and str(cod_id).strip(),
                        'Primary COD assignment requires a nonempty string or integer identifier')
                affected_cod.add(str(value['cod_id']))
    require({identifier for result in results for identifier in result['evidence_ids']} ==
            inserted.get('material_evidence', set()), 'Unexpected evidence inserts')
    sources = {}
    for record in records:
        for source in record['sources']:
            sources.setdefault(source['url'], source)
    for source_id in inserted.get('evidence_sources', set()):
        source = connection.execute('SELECT * FROM evidence_sources WHERE id=?', (source_id,)).fetchone()
        require(source and json.loads(source['metadata_json']) == sources.get(source['canonical_url']),
                'Inserted source differs from accepted citation')
    run = connection.execute('SELECT * FROM ingestion_runs WHERE id=?', (run_id,)).fetchone()
    require(run and run['source_label'] == batch and run['status'] == 'completed' and
            json.loads(run['metadata_json']).get('payload_sha256') == payload_hash, 'Saved ingestion mismatch')
    check_cod_assignments(connection, affected_cod)


def check_properties(original, updated):
    appended = ('publication_observations', 'publication_review_history', 'research_draft_descriptions')
    require({k: v for k, v in original.items() if k not in appended} ==
            {k: v for k, v in updated.items() if k not in appended}, 'Unrelated mineral properties changed')
    for key in appended:
        old = original.get(key, [])
        require(isinstance(old, list) and updated[key][:len(old)] == old, 'Prior publication state changed: ' + key)


def write_records(connection, records, batch, now, verification, inserted):
    results, source_ids, originals, intended_evidence = [], {}, {}, {}
    for record in records:
        row = connection.execute('SELECT * FROM materials WHERE public_id=?', (record['material_public_id'],)).fetchone()
        require(row and row['canonical_name'] == record['canonical_name'], 'Live mineral identity mismatch')
        originals[row['id']] = dict(row)
    for record in records:
        original = next(row for row in originals.values() if row['public_id'] == record['material_public_id'])
        identifier = original['id']
        before = json.loads(original['properties_json'])
        prior_ids = {c.get('claim_id') for c in before.get('publication_observations', [])}
        require(not prior_ids.intersection(c['claim_id'] for c in record['claims']),
                'Claim IDs already exist in the mineral; reconcile without duplicating prior observations')
        require(not any(h.get('review_batch') == batch for h in before.get('publication_review_history', [])), 'Do not replay a saved batch')
        updated = copy.deepcopy(before)
        record_sources = {s['url']: s for s in record['sources']}
        for url, source in record_sources.items():
            if url not in source_ids:
                found = connection.execute('SELECT id FROM evidence_sources WHERE canonical_url=?', (url,)).fetchone()
                if found:
                    source_ids[url] = found['id']
                else:
                    captures = source['validated_captures']
                    fingerprint = 'sha256:' + captures[0]['sha256'] if captures else ''
                    cursor = connection.execute('INSERT INTO evidence_sources(canonical_url,title,publisher,license_spdx,retrieved_at,content_hash,metadata_json) VALUES(?,?,?,?,?,?,?)',
                        (url, source['title'], source['publisher'], source['license_spdx'], source.get('retrieved_at', now), fingerprint, encoded(source)))
                    source_ids[url] = cursor.lastrowid
                    inserted.setdefault('evidence_sources', set()).add(cursor.lastrowid)
        observations = []
        for claim in record['claims']:
            observation = dict(claim)
            observation.update({'review_status': 'reviewed', 'reviewed_at': now, 'review_batch': batch,
                                })
            observation.setdefault('source_scope', 'reported_publication_observation')
            observation.setdefault('source_access', record_sources[claim['source_url']]['access'])
            observations.append(observation)
        updated.setdefault('publication_observations', []).extend(observations)
        draft = dict(record['editorial_draft'])
        draft.update({'private_research': True, 'publication_status': 'private', 'review_status': 'draft',
                      'review_batch': batch, 'created_at': now})
        updated.setdefault('research_draft_descriptions', []).append(draft)
        updated.setdefault('publication_review_history', []).append({
            'review_batch': batch, 'reviewed_at': now, 'review_note': record['review_note'],
            'sources': record['sources'], 'remaining_gaps': record['remaining_gaps'],
            'deferred_claims': record.get('deferred_claims', []), 'remaining_candidates': record.get('remaining_candidates', []),
            'coordinator_verification': verification, 'publication_status': 'private', 'public_ingestion_allowed': False})
        check_properties(before, updated)
        intended = dict(original, properties_json=encoded(updated),
                        search_text=original['search_text'] + ' ' + encoded(observations))
        connection.execute('UPDATE materials SET properties_json=?,search_text=? WHERE id=?',
                           (intended['properties_json'], intended['search_text'], identifier))
        bundles = {}
        for observation in observations:
            bundles.setdefault((observation['source_url'], observation['question_key']), []).append(observation)
        evidence_ids = []
        for (url, question), members in bundles.items():
            source, source_id = record_sources[url], source_ids[url]
            scope = question
            if connection.execute('SELECT 1 FROM material_evidence WHERE material_id=? AND source_id=? AND claim_scope=?',
                                  (identifier, source_id, scope)).fetchone():
                scope = 'private.content.' + batch + '.' + question
            bundle = {'question_key': question, 'value': members[0]['value'] if len(members) == 1 else {'reported_observations': members},
                      'unit': members[0].get('unit') if len(members) == 1 else None,
                      'source_locator': '; '.join(x['locator'] for x in members),
                      'note': members[0]['qualification'] if len(members) == 1 else 'Source-scoped observations retain their individual methods, locators and qualifications.',
                      'source_scope': 'reported_publication_observation', 'source_access': source['access'],
                      'review_batch': batch, 'observation_ids': [x['claim_id'] for x in members]}
            if len(members) == 1:
                bundle.update({key: members[0][key] for key in ('conditions', 'subject_scope', 'production_kind', 'evidence_level') if key in members[0]})
            captures = source['validated_captures']
            fingerprint = 'sha256:' + captures[0]['sha256'] if captures else ''
            values = {'material_id': identifier, 'source_id': source_id, 'claim_scope': scope,
                      'claim_json': encoded(bundle), 'confidence': 0.5, 'review_status': 'reviewed',
                      'source_title': source['title'], 'source_publisher': source['publisher'],
                      'source_license_spdx': source['license_spdx'], 'source_retrieved_at': source.get('retrieved_at', now),
                      'source_content_hash': fingerprint, 'dataset_key': 'mineral-content-queue-v1', 'source_release_id': batch}
            cursor = connection.execute('INSERT INTO material_evidence(' + ','.join(values) + ') VALUES(' +
                                        ','.join('?' for _ in values) + ')', tuple(values.values()))
            intended_evidence[cursor.lastrowid] = values
            evidence_ids.append(cursor.lastrowid)
            inserted.setdefault('material_evidence', set()).add(cursor.lastrowid)
        results.append({'material_public_id': record['material_public_id'], 'canonical_name': record['canonical_name'],
                        'material_sha256': row_hash(intended),
                        'observations': len(observations), 'evidence_rows': len(evidence_ids), 'evidence_ids': evidence_ids,
                        'source_works': len(record['sources']), 'deferred_candidates': len(record.get('deferred_claims', [])),
                        'remaining_candidates_preserved': len(record.get('remaining_candidates', [])), 'private_editorial_drafts': 1})
    return results, intended_evidence


def save_worker(queue, worker, confirm_reviewed=False, max_records=12, fault=None, wait_for_lock=True):
    """Save only this worker's immutable reviews; confirmation is scientific, not automatic."""
    require(confirm_reviewed is True, 'Scientifically verify your submitted reviews, then pass --confirm-reviewed')
    require(isinstance(worker, str) and worker.strip(), 'A stable worker ID is required')
    require(isinstance(max_records, int) and 1 <= max_records <= 100, 'max_records must be 1–100')
    with save_lock(queue, wait=wait_for_lock) as acquired:
        if not acquired:
            return {'result': 'busy', 'completed': [], 'submissions_retained': True,
                    'next_step': 'Another batch is saving. Continue your next queue record and retry later; '
                                 'use --wait-for-lock only when finishing or recovering your session.'}
        recovered = recover_worker(queue, worker)
        jobs, records = submitted_records(queue, worker, max_records)
        if not jobs:
            return {'result': 'no_submissions', 'completed': [], 'recovered': recovered}
        require(len({r['material_public_id'] for r in records}) == len(records), 'Duplicate mineral IDs')
        now = stamp()
        batch = 'content-save-' + datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%S') + '-' + uuid.uuid4().hex
        folder = queue.work / 'saves' / batch
        folder.mkdir(parents=True, exist_ok=False)
        verification = {'manual_review': True, 'reviewer_worker': worker,
                        'verified_records': [{'material_public_id': r['material_public_id'], 'canonical_name': r['canonical_name']} for r in records],
                        'accepted_observations': sum(len(r['claims']) for r in records)}
        payload = {'batch_id': batch, 'coordinator_verified': verification, 'records': records}
        payload_path, receipt_path = folder / 'accepted-reviews.json', folder / 'commit-receipt.json'
        write_json(payload_path, payload)
        payload_hash = file_hash(payload_path)
        public_before = public_state(queue.root)
        connection = sqlite3.connect(queue.data / 'minerals.db', timeout=300)
        connection.row_factory = sqlite3.Row
        connection.execute('PRAGMA foreign_keys=ON')
        connection.execute('PRAGMA synchronous=FULL')
        try:
            connection.execute('BEGIN IMMEDIATE')
            checkpoint = shared_checkpoint(queue, now)
            inserted = {}
            with batch_write_guard(connection):
                results, intended_evidence = write_records(connection, records, batch, now, verification, inserted)
                cursor = connection.execute('INSERT INTO ingestion_runs(source_label,status,item_count,imported_count,evidence_count,completed_at,metadata_json) VALUES(?,?,?,?,?,?,?)',
                    (batch, 'completed', len(records), len(records), sum(r['evidence_rows'] for r in results), now,
                     encoded({'manual_research': True, 'coordinator_verified': True, 'public_ingestion_allowed': False,
                              'payload_sha256': payload_hash, 'results': results,
                              'validation_scope': 'changed_records', 'batch_checks': BATCH_CHECKS})))
                run_id = cursor.lastrowid
                validate_batch(connection, records, results, batch, run_id, payload_hash, inserted, intended_evidence)
            require(public_state(queue.root) == public_before, 'Public catalog changed during private save')
            receipt = {'status': 'completed', 'committed_at': stamp(), 'reviewed_at': now, 'review_batch': batch,
                       'receipt_version': 2, 'validation_scope': 'changed_records', 'batch_checks': BATCH_CHECKS,
                       'receipt_stage': 'verified_before_commit; committed ingestion readback required',
                       'ingestion_run_id': run_id, 'completed_material_ids': [r['material_public_id'] for r in records],
                       'queue_claims': [{'public_id': j['public_id'], 'worker': worker, 'claim_token': j['token'],
                                         'submission_sha256': j['submission_sha256']} for j in jobs],
                       'results': results, 'observations_added': verification['accepted_observations'],
                       'evidence_rows_added': sum(r['evidence_rows'] for r in results),
                       'source_rows_added': len(inserted.get('evidence_sources', [])),
                       **checkpoint, 'payload_sha256': payload_hash, 'public_research_and_catalog_unchanged': True,
                       'source_access_and_rights': [{'material_public_id': r['material_public_id'], 'url': s['url'],
                             'access': s['access'], 'license_spdx': s['license_spdx']} for r in records for s in r['sources']]}
            # Reserve only the submitted rows' final check briefly; other sessions can
            # claim/research normally during the expensive database verification.
            with queue.transaction():
                for job in jobs:
                    current = queue.db.execute('SELECT * FROM jobs WHERE public_id=?', (job['public_id'],)).fetchone()
                    require(current and current['status'] == 'submitted' and
                            all(current[k] == job[k] for k in ('worker', 'token', 'submission_path', 'submission_sha256')),
                            'Queue assignment changed while preparing this save')
                    require(file_hash(queue.private_file(current['submission_path'])) == current['submission_sha256'],
                            'Submitted bytes changed while preparing this save')
                write_json(receipt_path, receipt)
                if fault:
                    fault('before_commit')
                connection.commit()
            if fault:
                fault('after_commit')
        except BaseException:
            connection.rollback()
            raise
        finally:
            connection.close()
        completed = [queue.complete(job['public_id'], receipt_path)['public_id'] for job in jobs]
        return {'result': 'saved', 'receipt_path': str(receipt_path), 'review_batch': batch,
                'completed': completed, 'recovered': recovered, 'observations_added': verification['accepted_observations']}


def load_queue_class():
    spec = importlib.util.spec_from_file_location('mineral_content_queue_save_dependency', Path(__file__).with_name('mineral-content-queue.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.ContentQueue


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--worker', required=True)
    parser.add_argument('--confirm-reviewed', action='store_true')
    parser.add_argument('--max-records', type=int, default=12)
    parser.add_argument('--wait-for-lock', action='store_true',
                        help='Wait for the current save when finishing; normally return busy and continue research')
    parser.add_argument('--root', type=Path, default=Path('/workspace'))
    parser.add_argument('--data', type=Path, default=Path('/app/data'))
    args = parser.parse_args()
    queue = None
    try:
        require(Path('/.dockerenv').exists(), 'Run inside the existing admin container through tools/container-task.sh')
        require(args.confirm_reviewed, 'Scientifically verify your reviews before --confirm-reviewed')
        if os.geteuid() == 0:
            owner = args.data.stat()
            os.setgroups([])
            os.setgid(owner.st_gid)
            os.setuid(owner.st_uid)
        os.umask(0o077)
        queue = load_queue_class()(root=args.root, data=args.data)
        print(json.dumps(save_worker(queue, args.worker, args.confirm_reviewed, args.max_records,
                                     wait_for_lock=args.wait_for_lock), ensure_ascii=False, indent=2))
        return 0
    except (ValueError, KeyError, TypeError, OSError, sqlite3.Error) as error:
        print(json.dumps({'error': str(error), 'submissions_retained': True}))
        return 1
    finally:
        if queue:
            queue.close()


if __name__ == '__main__':
    raise SystemExit(main())
