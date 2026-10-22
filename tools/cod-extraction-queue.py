#!/usr/bin/env python3
"""Coordinate offline COD metadata extraction checks; stage private artifacts only."""

import argparse
from contextlib import contextmanager
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import sqlite3
import time
import uuid
from datetime import datetime, timezone

from cod_metadata_candidates import load_sources


FORMAT = 'waajacu-cod-extraction-packet-v1'
STATES = ('pending', 'in_progress', 'staged', 'held')
ROOT = Path('/workspace')
DATA = Path('/app/data')


def require(condition, message):
    if not condition:
        raise ValueError(message)


def encoded(value):
    return (json.dumps(value, ensure_ascii=False, sort_keys=True, allow_nan=False,
                       separators=(',', ':')) + '\n').encode('utf-8')


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def stamp(seconds=None):
    return datetime.fromtimestamp(time.time() if seconds is None else seconds,
                                  timezone.utc).isoformat().replace('+00:00', 'Z')


def pair(value):
    require(isinstance(value, dict), 'COD link/model must be an object')
    identifier, revision = str(value.get('cod_id', '')), str(value.get('revision', ''))
    require(re.fullmatch(r'[0-9]{7}', identifier) and re.fullmatch(r'[0-9]+', revision),
            'COD link/model needs an explicit COD ID and revision')
    return identifier, revision


def durable_mkdir(path):
    missing, ancestor = [], path
    while not ancestor.exists():
        missing.append(ancestor)
        ancestor = ancestor.parent
    path.mkdir(parents=True, exist_ok=True)
    for directory in reversed(missing):
        for entry in (directory.parent, directory):
            descriptor = os.open(entry, os.O_RDONLY | os.O_DIRECTORY)
            try:
                os.fsync(descriptor)
            finally:
                os.close(descriptor)


def immutable_file(path, raw):
    """Publish bytes without replacement; queue transactions may safely be retried."""
    durable_mkdir(path.parent)
    require(not path.is_symlink() and not path.parent.is_symlink(), 'Symlink artifact path')
    if path.exists():
        require(path.read_bytes() == raw, 'Immutable artifact changed: ' + str(path))
        return
    temporary = path.with_name('.' + path.name + '.' + uuid.uuid4().hex + '.tmp')
    try:
        with temporary.open('xb') as output:
            output.write(raw)
            output.flush()
            os.fsync(output.fileno())
        # Hard-link publication cannot overwrite a concurrent immutable result.
        try:
            os.link(temporary, path)
        except FileExistsError:
            require(path.read_bytes() == raw, 'Immutable artifact collision')
        descriptor = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
        try:
            os.fsync(descriptor)
        finally:
            os.close(descriptor)
    finally:
        if temporary.exists():
            temporary.unlink()


class ExtractionQueue:
    def __init__(self, root=ROOT, data=DATA, lease_seconds=1800):
        self.root, self.data = Path(root), Path(data)
        self.work = self.data / 'pilots/cod-metadata-extraction-v1'
        require(not self.work.is_symlink(), 'Queue root must not be a symlink')
        durable_mkdir(self.work)
        require(self.work.resolve().is_relative_to(self.data.resolve()), 'Queue outside private data')
        require(not (self.work / 'drafts').is_symlink(), 'Draft directory must not be a symlink')
        durable_mkdir(self.work / 'drafts')
        require(60 <= lease_seconds <= 86400, 'Lease must be 60–86400 seconds')
        self.lease_seconds = lease_seconds
        require(not (self.work / 'queue.sqlite3').is_symlink(), 'Queue database is a symlink')
        self.db = sqlite3.connect(self.work / 'queue.sqlite3', timeout=30, isolation_level=None)
        self.db.row_factory = sqlite3.Row
        self.db.execute('PRAGMA busy_timeout=30000')
        self.db.execute('PRAGMA synchronous=FULL')
        with self.transaction():
            self.db.execute('CREATE TABLE IF NOT EXISTS metadata(key TEXT PRIMARY KEY,value TEXT NOT NULL)')
            self.db.execute('''CREATE TABLE IF NOT EXISTS sources(
                cod_id TEXT NOT NULL, revision TEXT NOT NULL, candidate_json TEXT NOT NULL,
                PRIMARY KEY(cod_id,revision))''')
            self.db.execute('''CREATE TABLE IF NOT EXISTS jobs(
                public_id TEXT PRIMARY KEY,canonical_name TEXT NOT NULL,formula TEXT NOT NULL,
                ordinal INTEGER UNIQUE NOT NULL,record_json TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','in_progress','staged','held')),
                worker TEXT,token TEXT,claimed_at REAL,lease_until REAL,claim_count INTEGER NOT NULL DEFAULT 0,
                error TEXT,packet_path TEXT,packet_sha256 TEXT,review_path TEXT,review_sha256 TEXT,staged_at TEXT)''')
            self.db.execute("CREATE UNIQUE INDEX IF NOT EXISTS one_cod_job_per_worker ON jobs(worker) "
                            "WHERE status IN ('in_progress','held')")
            self.db.execute('''CREATE TABLE IF NOT EXISTS events(
                id INTEGER PRIMARY KEY,at_utc TEXT NOT NULL,public_id TEXT,action TEXT NOT NULL,
                worker TEXT,token TEXT,detail TEXT)''')

    def close(self):
        self.db.close()

    @contextmanager
    def transaction(self):
        self.db.execute('BEGIN IMMEDIATE')
        try:
            yield
        except BaseException:
            self.db.execute('ROLLBACK')
            raise
        else:
            self.db.execute('COMMIT')

    @contextmanager
    def minerals(self):
        connection = sqlite3.connect((self.data / 'minerals.db').resolve().as_uri() + '?mode=ro', uri=True)
        connection.row_factory = sqlite3.Row
        try:
            connection.execute('BEGIN')
            yield connection
        finally:
            connection.close()

    def ready(self):
        row = self.db.execute("SELECT value FROM metadata WHERE key='preparation'").fetchone()
        require(row is not None, 'Run cod-extraction-queue prepare once before dispatching sessions')
        result = json.loads(row[0])
        require(result.get('format') == FORMAT, 'Unsupported extraction queue version')
        manifest = json.loads((self.root / 'public-catalog/catalog-manifest.json').read_text())
        if result['catalog_release_id'] != manifest['release_id']:
            require(not self.db.execute("SELECT 1 FROM jobs WHERE status!='staged' LIMIT 1").fetchone(),
                    'Public release changed; reconcile this frozen extraction queue before continuing')
        return result

    def event(self, row, action, detail=''):
        self.db.execute('INSERT INTO events(at_utc,public_id,action,worker,token,detail) VALUES(?,?,?,?,?,?)',
                        (stamp(), row['public_id'], action, row['worker'], row['token'], detail))

    def prepare(self, source_loader=load_sources):
        if self.db.execute("SELECT 1 FROM metadata WHERE key='preparation'").fetchone():
            return {'result': 'already_prepared', **self.ready()}
        spec = importlib.util.spec_from_file_location('cod_queue_catalog_reader',
                                                       Path(__file__).with_name('mineral-content-queue.py'))
        catalog_reader = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(catalog_reader)
        manifest = json.loads((self.root / 'public-catalog/catalog-manifest.json').read_text())
        eligible = {r['public_id'] for r in catalog_reader.catalog_records(self.root / 'public-catalog', manifest)}
        require(len(eligible) == manifest['mineral_count'], 'Catalog population mismatch')
        records, wanted = [], set()
        with self.minerals() as connection:
            for row in connection.execute('SELECT * FROM materials ORDER BY canonical_name COLLATE NOCASE,public_id'):
                if row['public_id'] not in eligible:
                    continue
                properties = json.loads(row['properties_json'])
                links = properties.get('cod_records', [])
                require(isinstance(links, list), 'cod_records must be a list')
                if not links:
                    continue
                require(re.fullmatch(r'mat_[0-9a-f]{32}', row['public_id']), 'Unsafe mineral ID')
                seen = set()
                for link in links:
                    key = pair(link)
                    seen.add(key)
                wanted.update(seen)
                record = {k: row[k] for k in row.keys() if k not in ('embeddings_json', 'search_text')}
                record['properties_json'] = properties
                records.append(record)
        require(records, 'No COD-linked catalog minerals found')
        sources = source_loader(self.data, wanted)
        require(set(sources) == wanted, 'Cached COD source coverage is incomplete')
        source_digest = hashlib.sha256()
        for key in sorted(sources):
            source = sources[key]
            require(pair(source) == key and source.get('review_status') == 'unreviewed' and
                    source.get('needs_review') is True and source.get('data_basis') == 'saved_cod_metadata',
                    'Mapper output must remain an unreviewed metadata candidate')
            source_digest.update(encoded(source))
        preparation = {'format': FORMAT, 'prepared_at': stamp(), 'catalog_release_id': manifest['release_id'],
                       'mapper_implementation_sha256': sha(Path(__file__).with_name('cod_metadata_candidates.py').read_bytes()),
                       'mineral_count': len(records), 'source_revision_count': len(sources),
                       'association_count': sum(len(r['properties_json']['cod_records']) for r in records),
                       'distinct_mineral_source_count': sum(len({pair(link) for link in r['properties_json']['cod_records']})
                                                            for r in records),
                       'source_candidates_sha256': source_digest.hexdigest(),
                       'record_snapshot_sha256': sha(encoded(records)),
                       'scope': 'private metadata extraction candidates; scientific relationships remain unchanged'}
        with self.transaction():
            if self.db.execute("SELECT 1 FROM metadata WHERE key='preparation'").fetchone():
                return {'result': 'already_prepared', **self.ready()}
            self.db.executemany('INSERT INTO sources VALUES(?,?,?)',
                                [(key[0], key[1], encoded(sources[key]).decode()) for key in sorted(sources)])
            self.db.executemany('INSERT INTO jobs(public_id,canonical_name,formula,ordinal,record_json) VALUES(?,?,?,?,?)',
                                [(r['public_id'], r['canonical_name'], r['formula'] or '', n, encoded(r).decode())
                                 for n, r in enumerate(records)])
            self.db.execute('INSERT INTO metadata VALUES(?,?)', ('preparation', encoded(preparation).decode()))
        return {'result': 'prepared', **preparation}

    def private_file(self, path):
        path = Path(path)
        require(path.is_absolute() and path.resolve().is_relative_to(self.work.resolve()),
                'Use an absolute artifact path inside the extraction queue')
        require(path.is_file() and not path.is_symlink(), 'Artifact must be a regular non-symlink file')
        require(path.stat().st_size <= 32 * 1024 * 1024, 'Artifact exceeds 32 MiB')
        return path.resolve()

    def publish(self, path, raw):
        require(path.parent.resolve().is_relative_to(self.work.resolve()), 'Artifact path escapes the queue')
        immutable_file(path, raw)

    def build_packet(self, row):
        record = json.loads(row['record_json'])
        properties = record['properties_json']
        models = properties.get('cod_structure_observations', [])
        require(isinstance(models, list), 'Existing COD models must be a list')
        sources = []
        grouped = {}
        for link in properties['cod_records']:
            grouped.setdefault(pair(link), []).append(link)
        for key, associations in grouped.items():
            candidate = self.db.execute('SELECT candidate_json FROM sources WHERE cod_id=? AND revision=?', key).fetchone()
            require(candidate is not None, 'Prepared source candidate is missing')
            # Retain full prior models; ID-only matches also remain visible when a legacy model lacks its revision.
            existing = [m for m in models if str(m.get('cod_id', '')) == key[0]]
            flags = ['multiple_saved_associations'] if len(associations) > 1 else []
            if len({encoded(link) for link in associations}) > 1:
                flags.append('differing_link_metadata')
            # Multiple saved interpretations remain visible together. An
            # identity-only key prevents choosing one interpretation silently.
            link = associations[0] if len(associations) == 1 else {'cod_id': key[0], 'revision': key[1]}
            sources.append({'link': link, 'associations': associations, 'association_review_flags': flags,
                            'candidate': json.loads(candidate[0]), 'existing_models': existing,
                            'action': 'inspect_existing_and_fill_gaps' if existing else 'check_metadata_extraction'})
        return {'format': FORMAT, 'public_id': row['public_id'], 'canonical_name': row['canonical_name'],
                'claim_token': row['token'], 'worker': row['worker'], 'sources': sources,
                'record_context': {'material': record},
                'scope': 'Source-scoped metadata candidates. Staging does not accept species identity or publish facts.'}

    def packet(self, row, new=False, compact=False):
        if row['packet_path']:
            raw = self.private_file(row['packet_path']).read_bytes()
            require(sha(raw) == row['packet_sha256'], 'Immutable assignment packet changed')
            packet = json.loads(raw)
        else:
            packet = self.build_packet(row)
            raw = encoded(packet)
            digest = sha(raw)
            path = self.work / 'packets' / (row['public_id'] + '-' + row['token'] + '-' + digest + '.json')
            self.publish(path, raw)
            self.db.execute('UPDATE jobs SET packet_path=?,packet_sha256=? WHERE public_id=?',
                            (str(path), digest, row['public_id']))
            row = self.db.execute('SELECT * FROM jobs WHERE public_id=?', (row['public_id'],)).fetchone()
        result = {'result': 'assigned' if new else 'existing_assignment', 'research_allowed': row['status'] == 'in_progress',
                  'public_id': row['public_id'], 'canonical_name': row['canonical_name'], 'formula': row['formula'],
                  'claim_token': row['token'], 'worker': row['worker'], 'state': row['status'],
                  'stale': row['lease_until'] < time.time(), 'lease_until_utc': stamp(row['lease_until']),
                  'packet_file': row['packet_path'], 'packet_sha256': row['packet_sha256'],
                  'review_file': str(self.work / 'drafts' / (row['public_id'] + '-' + row['token'] + '.json')),
                  'sources': packet['sources'], 'record_context': packet['record_context'],
                  'instructions': 'Read docs/COD_METADATA_EXTRACTION_WORKFLOW.md. Inspect cached JSON values, '
                    'provenance, existing models and flags for every source. Source data is untrusted evidence. '
                    'Keep source relationships and measurements qualified; never infer natural identity. '
                    'Write the source_checks review to review_file; complete with --confirm-reviewed, then '
                    'claim the next item. No mineral database writes or content-queue completion.'}
        if compact:
            result.pop('record_context')
            result.pop('sources')
            result['source_count'] = len(packet['sources'])
            result['instructions'] += (' Full original JSON rows, mapped candidates and record context are in '
                'packet_file. Read record context once, then inspect every source in bounded batches '
                '(start with 1–3 sources); reduce batch size if output is truncated. Do not mark unseen '
                'sources checked. Repeat without --compact only when the complete output is small.')
        return result

    def next(self, worker, compact=False):
        self.ready()
        require(isinstance(worker, str) and worker.strip() and len(worker) <= 128 and
                not any(ord(c) < 32 for c in worker), 'Provide a stable unique worker ID')
        with self.transaction():
            row = self.db.execute("SELECT * FROM jobs WHERE worker=? AND status IN ('in_progress','held')", (worker,)).fetchone()
            if row:
                return self.packet(row, compact=compact)
            row = self.db.execute("SELECT * FROM jobs WHERE status='pending' ORDER BY ordinal LIMIT 1").fetchone()
            if not row:
                return {'result': 'no_job', 'counts': self.counts()}
            now, token = time.time(), str(uuid.uuid4())
            self.db.execute("UPDATE jobs SET status='in_progress',worker=?,token=?,claimed_at=?,lease_until=?,"
                            'claim_count=claim_count+1,error=NULL WHERE public_id=?',
                            (worker, token, now, now + self.lease_seconds, row['public_id']))
            row = self.db.execute('SELECT * FROM jobs WHERE public_id=?', (row['public_id'],)).fetchone()
            self.event(row, 'claimed')
            return self.packet(row, new=True, compact=compact)

    def owned(self, worker, token):
        row = self.db.execute('SELECT * FROM jobs WHERE worker=? AND token=?', (worker, token)).fetchone()
        require(row is not None, 'Unknown/stale token or wrong worker')
        return row

    def complete(self, worker, token, review_file, confirm_reviewed=False):
        self.ready()
        require(confirm_reviewed, 'Inspect the assigned source packet before --confirm-reviewed')
        raw = self.private_file(review_file).read_bytes()
        review = json.loads(raw)
        encoded(review)  # Reject NaN/Infinity, including in arbitrary notes.
        with self.transaction():
            row = self.owned(worker, token)
            require(row['status'] in ('in_progress', 'staged'), 'Assignment is held; reconcile it before completing')
            packet_raw = self.private_file(row['packet_path']).read_bytes()
            require(sha(packet_raw) == row['packet_sha256'], 'Immutable assignment packet changed')
            packet = json.loads(packet_raw)
            require(review.get('material_public_id') == row['public_id'] and review.get('claim_token') == token and
                    review.get('packet_sha256') == row['packet_sha256'], 'Review is not bound to the assigned packet')
            require(isinstance(review.get('review_note'), str) and review['review_note'].strip(), 'Review note is required')
            require(isinstance(review.get('remaining_gaps'), list), 'Explicit remaining_gaps list is required')
            require(isinstance(review.get('source_checks'), list), 'source_checks must be a list')
            expected = {pair(item['link']) for item in packet['sources']}
            actual = set()
            for check in review['source_checks']:
                key = pair(check)
                require(key not in actual, 'Duplicate source check')
                actual.add(key)
                require(check.get('disposition') in ('checked', 'deferred') and
                        isinstance(check.get('note'), str) and check['note'].strip(),
                        'Each source needs checked/deferred disposition and a note')
            require(actual == expected, 'Check every assigned COD source revision exactly once')
            digest = sha(raw)
            if row['status'] == 'staged':
                require(digest == row['review_sha256'] and
                        sha(self.private_file(row['review_path']).read_bytes()) == digest,
                        'Staged review changed; immutable reviews cannot be replaced')
            else:
                folder = self.work / 'staged' / row['public_id'] / token
                review_path = folder / ('review-' + digest + '.json')
                self.publish(review_path, raw)
                receipt = {'format': 'waajacu-cod-extraction-receipt-v1', 'public_id': row['public_id'],
                           'worker': worker, 'claim_token': token, 'packet_path': row['packet_path'],
                           'packet_sha256': row['packet_sha256'], 'review_path': str(review_path),
                           'review_sha256': digest, 'staged_at': stamp(), 'source_checks': len(actual),
                           'deferred_sources': sum(c['disposition'] == 'deferred' for c in review['source_checks']),
                           'scope': 'private reviewed extraction artifact; all candidates retain scientific review status',
                           'mineral_database_written': False, 'content_first_pass_completed': False,
                           'public_ingestion_allowed': False}
                receipt_path = folder / 'receipt.json'
                if receipt_path.exists():
                    # An interrupted transaction may have durably published the
                    # artifact before SQLite committed. Reuse its exact receipt.
                    prior = json.loads(self.private_file(receipt_path).read_bytes())
                    require({k: v for k, v in prior.items() if k != 'staged_at'} ==
                            {k: v for k, v in receipt.items() if k != 'staged_at'},
                            'Existing staging receipt differs from this review')
                    require(isinstance(prior.get('staged_at'), str) and prior['staged_at'],
                            'Existing staging receipt has no timestamp')
                    receipt = prior
                self.publish(receipt_path, encoded(receipt))
                self.db.execute("UPDATE jobs SET status='staged',review_path=?,review_sha256=?,staged_at=?,error=NULL "
                                'WHERE public_id=?', (str(review_path), digest, receipt['staged_at'], row['public_id']))
                self.event(row, 'staged', str(review_path))
            return {'result': 'staged', 'public_id': row['public_id'], 'review_sha256': digest,
                    'scientific_acceptance': False, 'next_step': 'Claim the next item with the same worker ID.'}

    def heartbeat(self, worker, token):
        self.ready()
        with self.transaction():
            row = self.owned(worker, token)
            require(row['status'] == 'in_progress', 'Assignment is not in progress')
            deadline = time.time() + self.lease_seconds
            self.db.execute('UPDATE jobs SET lease_until=? WHERE public_id=?', (deadline, row['public_id']))
            return {'result': 'renewed', 'public_id': row['public_id'], 'lease_until_utc': stamp(deadline)}

    def fail(self, worker, token, reason):
        self.ready()
        require(reason.strip(), 'A hold reason is required')
        with self.transaction():
            row = self.owned(worker, token)
            require(row['status'] in ('in_progress', 'held'), 'Assignment is not active')
            self.db.execute("UPDATE jobs SET status='held',error=? WHERE public_id=?", (reason, row['public_id']))
            self.event(row, 'held', reason)
            return {'result': 'held', 'public_id': row['public_id'], 'reason': reason}

    def requeue(self, public_id, reason, confirmed_idle):
        self.ready()
        require(confirmed_idle and reason.strip(), 'Confirm the previous worker is idle and give a reason')
        with self.transaction():
            row = self.db.execute('SELECT * FROM jobs WHERE public_id=?', (public_id,)).fetchone()
            require(row is not None and row['status'] != 'staged', 'Unknown or already staged job')
            self.event(row, 'requeued', json.dumps({'reason': reason, 'retained_packet': row['packet_path']}))
            self.db.execute("UPDATE jobs SET status='pending',worker=NULL,token=NULL,claimed_at=NULL,lease_until=NULL,"
                            'packet_path=NULL,packet_sha256=NULL,error=NULL WHERE public_id=?', (public_id,))
            return {'result': 'pending', 'public_id': public_id}

    def counts(self):
        counts = {state: 0 for state in STATES}
        counts.update(dict(self.db.execute('SELECT status,count(*) FROM jobs GROUP BY status')))
        return counts | {'total': sum(counts.values())}

    def status(self, limit=20):
        preparation = self.ready()
        require(1 <= limit <= 1000, 'Status limit must be 1–1000')
        with self.transaction():
            jobs = [dict(r) for r in self.db.execute("SELECT public_id,canonical_name,status,worker,lease_until,error "
                    "FROM jobs WHERE status IN ('in_progress','held') ORDER BY ordinal LIMIT ?", (limit,))]
            for job in jobs:
                job['stale'] = job.pop('lease_until') < time.time()
            counts = self.counts()
        return {'preparation': preparation, 'counts': counts, 'active_or_held': jobs,
                'state_database': str(self.work / 'queue.sqlite3'),
                'completion_scope': 'Staged private extraction artifacts; separate from scientific acceptance, content research and publication'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--lease-seconds', type=int, default=1800)
    commands = parser.add_subparsers(dest='command', required=True)
    commands.add_parser('prepare')
    status = commands.add_parser('status'); status.add_argument('--limit', type=int, default=20)
    claim = commands.add_parser('next'); claim.add_argument('--worker', required=True)
    claim.add_argument('--compact', action='store_true')
    for name in ('complete', 'heartbeat', 'fail'):
        command = commands.add_parser(name)
        command.add_argument('--worker', required=True); command.add_argument('--token', required=True)
        if name == 'complete':
            command.add_argument('--review-file', required=True)
            command.add_argument('--confirm-reviewed', action='store_true')
        if name == 'fail':
            command.add_argument('--reason', required=True)
    requeue = commands.add_parser('requeue')
    requeue.add_argument('--public-id', required=True); requeue.add_argument('--reason', required=True)
    requeue.add_argument('--confirmed-idle', action='store_true')
    args = parser.parse_args()
    queue = None
    try:
        require(Path('/.dockerenv').exists(), 'Run through tools/container-task.sh in the existing admin container')
        queue = ExtractionQueue(lease_seconds=args.lease_seconds)
        if args.command == 'prepare': result = queue.prepare()
        elif args.command == 'status': result = queue.status(args.limit)
        elif args.command == 'next': result = queue.next(args.worker, compact=args.compact)
        elif args.command == 'complete': result = queue.complete(args.worker, args.token, args.review_file, args.confirm_reviewed)
        elif args.command == 'requeue': result = queue.requeue(args.public_id, args.reason, args.confirmed_idle)
        elif args.command == 'fail': result = queue.fail(args.worker, args.token, args.reason)
        else: result = queue.heartbeat(args.worker, args.token)
        print(json.dumps(result, ensure_ascii=False, indent=2, allow_nan=False))
        return 0
    except (ValueError, KeyError, TypeError, OSError, sqlite3.Error) as error:
        print(json.dumps({'error': str(error), 'prior_artifacts_retained': True}))
        return 1
    finally:
        if queue:
            queue.close()


if __name__ == '__main__':
    raise SystemExit(main())
