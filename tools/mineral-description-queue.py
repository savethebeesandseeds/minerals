#!/usr/bin/env python3
"""Hand out one record-description job at a time and save that summary.

Descriptions are written from facts already stored on the mineral. This queue
does not add sources, evidence rows, or content-research completions.
"""

import argparse
from contextlib import contextmanager
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import sqlite3
import time
import uuid

try:
    import fcntl
except ImportError:  # pragma: no cover - container and Linux tests have fcntl
    fcntl = None


ROOT = Path('/workspace')
DATA = Path('/app/data')
STATES = ('pending', 'in_progress', 'complete', 'failed')
GROUPS = ('missing', 'thin', 'retain', 'rewrite')
GROUP_RANK = {name: index for index, name in enumerate(GROUPS)}
SCOPE_PREFIXES = (
    'physical.color', 'physical.crystal_habit', 'physical.hardness_mohs',
    'physical.luster', 'physical.streak', 'physical.transparency',
    'history.type_localities', 'geology.formation_environments', 'geology.host_rocks',
    'occurrence.', 'physical.', 'optical.', 'history.', 'geology.',
    'chemistry.', 'crystallography.', 'classification.', 'identity.',
)
CHECKPOINT_INTERVAL_SECONDS = 24 * 60 * 60
CHECKPOINT_RETENTION = 2
CHECKPOINT_DIRECTORY = re.compile(r'checkpoint-[0-9a-f]{32}')
URL_IN_TEXT = re.compile(r'https?://[^\s)>\]]+')


def require(value, message):
    if not value:
        raise ValueError(message)


def stamp(seconds=None):
    moment = datetime.now(timezone.utc) if seconds is None else datetime.fromtimestamp(seconds, timezone.utc)
    return moment.isoformat().replace('+00:00', 'Z')


def encoded(value):
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(',', ':'), allow_nan=False)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def file_hash(path):
    digest = hashlib.sha256()
    with path.open('rb') as content:
        while chunk := content.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def read_json(path):
    raw = path.read_bytes()
    require(len(raw) <= 1024 * 1024, 'JSON artifact exceeds 1 MiB')
    value = json.loads(raw)
    json.dumps(value, allow_nan=False)
    return value, raw


def sync_directory(path):
    descriptor = os.open(path, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def table_exists(connection, name):
    return connection.execute(
        "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?", (name,)).fetchone() is not None


def value_text(value):
    if isinstance(value, str):
        return value.strip()
    if isinstance(value, bool) or value is None:
        return ''
    if isinstance(value, (int, float)):
        return str(value)
    if isinstance(value, list):
        parts = []
        for item in value:
            text = value_text(item)
            if text and not text.startswith('{'):
                parts.append(text)
        return ', '.join(parts)
    if isinstance(value, dict):
        locality = value.get('reported_locality')
        if isinstance(locality, str) and locality.strip():
            return locality.strip()
        reported = value.get('reported')
        if isinstance(reported, (list, str)):
            text = value_text(reported)
            if text:
                return text
        for key in ('type_material', 'habit', 'value', 'text', 'name', 'system', 'crystal_system'):
            if isinstance(value.get(key), str) and value.get(key).strip():
                return value[key].strip()
        return ''
    return ''


def clip(text, limit=180):
    text = ' '.join(text.split())
    return text if len(text) <= limit else text[:limit - 1].rstrip() + '…'


def urls_in(value):
    found = set()
    if isinstance(value, dict):
        for item in value.values():
            found.update(urls_in(item))
    elif isinstance(value, list):
        for item in value:
            found.update(urls_in(item))
    elif isinstance(value, str) and value.startswith(('https://', 'http://')) and not re.search(r'\s', value):
        found.add(value)
    return found


def scope_rank(scope):
    for index, prefix in enumerate(SCOPE_PREFIXES):
        if scope.startswith(prefix):
            return index
    return len(SCOPE_PREFIXES)


def classify_description(column_text, research):
    research_text = ''
    urls = []
    has_object = isinstance(research, dict)
    if has_object:
        research_text = research.get('text').strip() if isinstance(research.get('text'), str) else ''
        urls = [url for url in research.get('source_urls', []) if isinstance(url, str)]
    text = research_text or column_text
    if not text:
        return 'missing', 'compose', '', []
    if len(text) < 120 or (has_object and research_text and not urls):
        return 'thin', 'compose', text, urls
    return 'retain', 'retain', text, urls


class DescriptionQueue:
    def __init__(self, root=ROOT, data=DATA, lease_seconds=1800):
        self.root, self.data = Path(root), Path(data)
        self.work = self.data / 'pilots/mineral-description-queue-v1'
        self.work.mkdir(parents=True, exist_ok=True)
        (self.work / 'drafts').mkdir(exist_ok=True)
        (self.work / 'receipts').mkdir(exist_ok=True)
        self.lease_seconds = lease_seconds
        require(60 <= lease_seconds <= 86400, 'Lease must be 60–86400 seconds')
        self.db = sqlite3.connect(self.work / 'queue.sqlite3', timeout=30, isolation_level=None)
        self.db.row_factory = sqlite3.Row
        self.db.execute('PRAGMA busy_timeout=30000')
        try:
            with self.transaction():
                self.db.execute('CREATE TABLE IF NOT EXISTS metadata (key TEXT PRIMARY KEY, value TEXT NOT NULL)')
                self.db.execute('''CREATE TABLE IF NOT EXISTS jobs (
                    public_id TEXT PRIMARY KEY, canonical_name TEXT NOT NULL, formula TEXT NOT NULL,
                    ordinal INTEGER NOT NULL, priority_group TEXT NOT NULL,
                    status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','in_progress','complete','failed')),
                    worker TEXT, token TEXT, claimed_at REAL, lease_until REAL,
                    claim_count INTEGER NOT NULL DEFAULT 0, error TEXT,
                    receipt_path TEXT, receipt_sha256 TEXT, completed_at TEXT)''')
                self.db.execute("CREATE UNIQUE INDEX IF NOT EXISTS one_description_job_per_worker ON jobs(worker) "
                                "WHERE status='in_progress'")
                self.db.execute('''CREATE TABLE IF NOT EXISTS events (
                    id INTEGER PRIMARY KEY, at_utc TEXT NOT NULL, public_id TEXT,
                    action TEXT NOT NULL, worker TEXT, token TEXT, detail TEXT)''')
                self.initialize()
        except BaseException:
            self.close()
            raise

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
    def minerals(self, writable=False):
        path = (self.data / 'minerals.db').resolve()
        connection = sqlite3.connect(path if writable else path.as_uri() + '?mode=ro', uri=not writable, timeout=300)
        connection.row_factory = sqlite3.Row
        connection.execute('PRAGMA busy_timeout=300000')
        if writable:
            connection.execute('PRAGMA foreign_keys=ON')
            connection.execute('PRAGMA synchronous=FULL')
        try:
            yield connection
        finally:
            connection.close()

    def private_file(self, path):
        path = Path(path)
        require(path.is_absolute(), 'Use an absolute private data path')
        resolved = path.resolve()
        require(resolved.is_relative_to(self.data.resolve()), 'Artifact is outside private data')
        require(path.is_file() and not path.is_symlink(), 'Artifact must be a regular non-symlink file')
        return resolved

    def event(self, row, action, detail=''):
        self.db.execute('INSERT INTO events(at_utc,public_id,action,worker,token,detail) VALUES(?,?,?,?,?,?)',
                        (stamp(), row['public_id'], action, row['worker'], row['token'], detail))

    def initialize(self):
        version = self.db.execute("SELECT value FROM metadata WHERE key='schema_version'").fetchone()
        if version:
            require(version[0] == '1', 'Unsupported description queue schema')
        else:
            self.db.execute("INSERT INTO metadata(key,value) VALUES('schema_version','1')")
            self.db.execute('INSERT INTO metadata(key,value) VALUES(?,?)', ('created_at', stamp()))
        with self.minerals() as connection:
            require(table_exists(connection, 'materials'), 'Private mineral database has no materials table')
            rows = list(connection.execute(
                "SELECT id, public_id, canonical_name, formula, description, properties_json "
                "FROM materials WHERE record_type='mineral' ORDER BY public_id"))
        existing = {row[0] for row in self.db.execute('SELECT public_id FROM jobs')}
        fresh = []
        for row in rows:
            if row['public_id'] in existing or not row['public_id']:
                continue
            properties = json.loads(row['properties_json'] or '{}')
            research = properties.get('public_research') if isinstance(properties, dict) else None
            description = research.get('description') if isinstance(research, dict) else None
            group, _action, _text, _urls = classify_description((row['description'] or '').strip(), description)
            fresh.append((group, row['canonical_name'].casefold(), row))
        fresh.sort(key=lambda item: (GROUP_RANK[item[0]], item[1], item[2]['public_id']))
        start = self.db.execute('SELECT COALESCE(MAX(ordinal), -1) FROM jobs').fetchone()[0] + 1
        # Keep the three groups in order even when new minerals appear later:
        # insert missing, then thin, then retain, but a later seed appends after
        # work already ordered by the first seed. The first seed is the catalogue.
        self.db.executemany(
            'INSERT INTO jobs(public_id, canonical_name, formula, ordinal, priority_group) VALUES(?,?,?,?,?)',
            [(row['public_id'], row['canonical_name'], row['formula'] or '', start + index, group)
             for index, (group, _name, row) in enumerate(fresh)])

    def counts(self):
        result = {state: 0 for state in STATES}
        result.update({row[0]: row[1] for row in self.db.execute('SELECT status, count(*) FROM jobs GROUP BY status')})
        result['total'] = sum(result[state] for state in STATES)
        result['stale_in_progress'] = self.db.execute(
            "SELECT count(*) FROM jobs WHERE status='in_progress' AND lease_until<?", (time.time(),)).fetchone()[0]
        return result

    def load_material(self, connection, public_id):
        material = connection.execute('SELECT * FROM materials WHERE public_id=?', (public_id,)).fetchone()
        require(material is not None, 'Assigned mineral is missing from the private database')
        properties = json.loads(material['properties_json'] or '{}')
        require(isinstance(properties, dict), 'Mineral properties must be a JSON object')
        facts = {}
        if table_exists(connection, 'mineral_dataset_facts'):
            for row in connection.execute(
                    'SELECT fact_key, fact_value FROM mineral_dataset_facts WHERE material_id=?', (material['id'],)):
                facts.setdefault(row['fact_key'], row['fact_value'])
        evidence = []
        if table_exists(connection, 'material_evidence'):
            evidence = [dict(row) for row in connection.execute(
                'SELECT * FROM material_evidence WHERE material_id=?', (material['id'],))]
        return material, properties, facts, evidence

    def known_facts(self, material, properties, facts, evidence):
        research = properties.get('public_research') if isinstance(properties.get('public_research'), dict) else {}
        description = research.get('description') if isinstance(research.get('description'), dict) else None
        group, action, text, urls = classify_description((material['description'] or '').strip(), description)
        claims = []
        allowed = set(urls)
        for row in evidence:
            if row.get('review_status') != 'reviewed':
                continue
            scope = row.get('claim_scope') or ''
            if scope.startswith('properties.cod_'):
                continue
            try:
                claim = json.loads(row.get('claim_json') or '{}')
            except (TypeError, ValueError):
                continue
            if not isinstance(claim, dict):
                continue
            text_value = clip(value_text(claim.get('value')))
            if not text_value:
                continue
            claim_urls = sorted(urls_in(claim) | urls_in(row.get('source_work_url')))
            allowed.update(claim_urls)
            qualification = claim.get('qualification') if isinstance(claim.get('qualification'), str) else ''
            if not qualification and isinstance(claim.get('note'), str):
                qualification = claim['note']
            claims.append({
                'scope': scope,
                'value': text_value,
                'qualification': clip(qualification, 120),
                'source_urls': claim_urls,
                'rank': scope_rank(scope),
                'id': row.get('id') or 0,
            })
        claims.sort(key=lambda item: (item['rank'], item['id']))
        kept = [{key: item[key] for key in ('scope', 'value', 'qualification', 'source_urls')}
                for item in claims[:24]]
        for item in kept:
            allowed.update(item['source_urls'])
        cod_records = research.get('cod_records') if isinstance(research.get('cod_records'), list) else []
        structures = research.get('structures') if isinstance(research.get('structures'), list) else []
        cod_links = sum(1 for row in evidence if (row.get('claim_scope') or '').startswith('properties.cod_'))
        return {
            'action': action,
            'priority_group': group,
            'canonical_name': material['canonical_name'],
            'formula': material['formula'] or '',
            'nomenclature_status': material['nomenclature_status'] if 'nomenclature_status' in material.keys() else '',
            'is_valid_species': material['is_valid_species'] if 'is_valid_species' in material.keys() else 1,
            'discovery_country': facts.get('discovery_country', ''),
            'first_reference': facts.get('first_reference', ''),
            'second_reference': facts.get('second_reference', ''),
            'existing_description': text,
            'existing_source_urls': urls,
            'reviewed_claims': kept,
            'reviewed_claim_count': len(claims),
            'cod_summary': {
                'public_cod_records': len(cod_records),
                'public_structure_models': len(structures),
                'cod_evidence_rows': cod_links,
                'note': 'COD measurements stay source-specific. Do not replace the mineral formula or species properties with them.',
            },
            'allowed_urls': sorted(allowed),
        }

    def packet(self, row, new=False):
        with self.minerals() as connection:
            material, properties, facts, evidence = self.load_material(connection, row['public_id'])
            known = self.known_facts(material, properties, facts, evidence)
        review = self.work / 'drafts' / (row['public_id'] + '-' + row['token'] + '.json')
        return {
            'result': 'assigned' if new else 'existing_assignment',
            'new_assignment': new,
            'public_id': row['public_id'],
            'canonical_name': row['canonical_name'],
            'formula': row['formula'],
            'state': row['status'],
            'priority_group': row['priority_group'],
            'worker': row['worker'],
            'claim_token': row['token'],
            'claim_count': row['claim_count'],
            'lease_until_utc': stamp(row['lease_until']) if row['lease_until'] else None,
            'stale': bool(row['lease_until'] and row['lease_until'] < time.time()),
            'review_file': str(review),
            'known': known,
            'instructions': (
                'Write a short header from known facts only. Do not search, open papers, or add sources. '
                'Use description-queue complete with this claim token. '
                'action compose: two or three sentences, 40–700 characters, in natural prose. '
                'Name the mineral and describe it as a specimen: color, crystal sharpness or habit, and the places it is found. '
                'Use hardness or luster only when that value is in known.reviewed_claims. '
                'discovery_country may be mentioned as where it was described, not as a labeled field. '
                'If color, habit, or a place is absent, leave it out. Do not say that the catalog lacks it. '
                'Do not inventory the record: no IMA status, no reference citations, no COD counts, no formula dump, no field list. '
                'source_urls must be chosen from known.allowed_urls. '
                'action retain: copy known.existing_description and known.existing_source_urls unchanged.'
            ),
        }

    def next(self, worker, priority_group=None):
        require(isinstance(worker, str) and worker.strip() and len(worker) <= 128
                and not any(ord(character) < 32 for character in worker),
                'Provide a stable nonempty worker ID')
        require(priority_group in (None, *GROUPS), 'Unknown priority group')
        with self.transaction():
            held = self.db.execute(
                "SELECT * FROM jobs WHERE worker=? AND status='in_progress'", (worker,)).fetchone()
            if held:
                require(priority_group is None or held['priority_group'] == priority_group,
                        'Worker holds a different priority group')
                return self.packet(held)
            row = self.db.execute(
                "SELECT * FROM jobs WHERE status='pending' AND (? IS NULL OR priority_group=?) "
                'ORDER BY ordinal LIMIT 1', (priority_group, priority_group)).fetchone()
            if not row:
                return {'result': 'no_job', 'counts': self.counts()}
            now, token = time.time(), str(uuid.uuid4())
            self.db.execute(
                "UPDATE jobs SET status='in_progress', worker=?, token=?, claimed_at=?, lease_until=?, "
                'claim_count=claim_count+1, error=NULL WHERE public_id=?',
                (worker, token, now, now + self.lease_seconds, row['public_id']))
            row = self.db.execute('SELECT * FROM jobs WHERE public_id=?', (row['public_id'],)).fetchone()
            self.event(row, 'claimed')
            return self.packet(row, True)

    def status(self):
        with self.transaction():
            groups = {}
            for group, state, count in self.db.execute(
                    'SELECT priority_group, status, count(*) FROM jobs GROUP BY priority_group, status'):
                groups.setdefault(group, {name: 0 for name in STATES})[state] = count
            for group in groups.values():
                group['total'] = sum(group[name] for name in STATES)
            active = [dict(row) for row in self.db.execute(
                "SELECT public_id, canonical_name, priority_group, worker FROM jobs "
                "WHERE status='in_progress' ORDER BY ordinal LIMIT 20")]
            return {'counts': self.counts(), 'priority_groups': groups, 'in_progress': active}

    def validate_review(self, record, known):
        require(record.get('material_public_id') == known['public_id_check'], 'Review mineral identity mismatch')
        require(record.get('canonical_name') == known['canonical_name'], 'Review mineral name mismatch')
        require(record.get('action') == known['action'], 'Review action does not match the claimed record')
        text = record.get('text')
        require(isinstance(text, str) and text.strip() == text, 'Description text must be an exact non-empty string')
        urls = record.get('source_urls')
        require(isinstance(urls, list) and all(isinstance(url, str) for url in urls), 'source_urls must be a list of strings')
        require(len(urls) == len(set(urls)), 'Duplicate source URL')
        allowed = set(known['allowed_urls'])
        require(set(urls) <= allowed, 'Description cites a URL that is not on the record')
        found = {match.rstrip('.,;') for match in URL_IN_TEXT.findall(text)}
        require(found <= allowed, 'Description text contains a URL that is not on the record')
        if known['action'] == 'retain':
            require(text == known['existing_description'], 'Retain must copy the existing description unchanged')
            require(urls == known['existing_source_urls'], 'Retain must copy the existing source URLs unchanged')
            return
        require(40 <= len(text) <= 700, 'Composed description must be 40–700 characters')
        require(known['canonical_name'].casefold() in text.casefold(), 'Composed description must name the mineral')
        lowered = text.casefold()
        for phrase in (
            'ima master list', 'crystallography open database', 'not in the catalog record',
            'reviewed catalog values', 'discovery country as', 'is an approved mineral with formula',
            'is a grandfathered mineral with formula', 'is a questionable mineral with formula',
        ):
            require(phrase not in lowered, 'Description reads as a catalog inventory, not a specimen header')

    def checkpoint_directories(self):
        root = self.work / 'database-checkpoints'
        require(not root.is_symlink(), 'Checkpoint directory must not be a symlink')
        existed = root.exists()
        root.mkdir(exist_ok=True)
        require(root.resolve().is_relative_to(self.data.resolve()), 'Checkpoint directory is outside private data')
        if not existed:
            sync_directory(self.work)
        folders = []
        for path in root.iterdir():
            if CHECKPOINT_DIRECTORY.fullmatch(path.name):
                require(path.is_dir() and not path.is_symlink(), 'Invalid managed checkpoint directory')
                folders.append(path)
        return root, folders

    def create_checkpoint(self, root, now):
        folder = root / ('checkpoint-' + uuid.uuid4().hex)
        folder.mkdir()
        temporary = folder / 'minerals.sqlite3.pending'
        backup = folder / 'minerals.sqlite3'
        try:
            source = sqlite3.connect((self.data / 'minerals.db').resolve().as_uri() + '?mode=ro', uri=True)
            try:
                destination = sqlite3.connect(temporary)
                try:
                    source.backup(destination)
                    require(destination.execute('PRAGMA integrity_check').fetchall() == [('ok',)],
                            'Description checkpoint integrity check failed')
                    require(not destination.execute('PRAGMA foreign_key_check').fetchall(),
                            'Description checkpoint foreign key check failed')
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
                        'bytes': info.st_size, 'mtime_ns': info.st_mtime_ns,
                        'audit': {'integrity': 'ok', 'foreign_key_violations': 0}}
            (folder / 'checkpoint.json').write_text(encoded(metadata), encoding='utf-8')
            sync_directory(folder)
            return folder, metadata
        except BaseException:
            if not (folder / 'checkpoint.json').exists():
                if temporary.exists():
                    temporary.unlink()
                if folder.exists():
                    for child in folder.iterdir():
                        child.unlink()
                    folder.rmdir()
            raise

    def shared_checkpoint(self, now):
        root, folders = self.checkpoint_directories()
        current = datetime.fromisoformat(now.replace('Z', '+00:00'))
        checkpoints = []
        for folder in folders:
            marker = folder / 'checkpoint.json'
            if not marker.is_file():
                continue
            metadata = json.loads(marker.read_text(encoding='utf-8'))
            created = datetime.fromisoformat(metadata['created_at'].replace('Z', '+00:00'))
            checkpoints.append((created, folder, metadata))
        checkpoints.sort(key=lambda item: (item[0], item[1].name), reverse=True)
        if checkpoints and (current - checkpoints[0][0]).total_seconds() < CHECKPOINT_INTERVAL_SECONDS:
            folder, metadata = checkpoints[0][1], checkpoints[0][2]
            backup = folder / 'minerals.sqlite3'
        else:
            folder, metadata = self.create_checkpoint(root, now)
            backup = folder / 'minerals.sqlite3'
            checkpoints.insert(0, (current, folder, metadata))
        keep = {item[1] for item in checkpoints[:CHECKPOINT_RETENTION]}
        for folder_path in folders:
            if folder_path not in keep and folder_path != folder:
                marker = folder_path / 'checkpoint.json'
                if marker.exists():
                    marker.unlink()
                for child in list(folder_path.iterdir()):
                    if child.is_file():
                        child.unlink()
                folder_path.rmdir()
        return {'backup_path': str(backup), 'backup_sha256': metadata['sha256'],
                'backup_policy': 'shared_daily_checkpoint', 'backup_created_at': metadata['created_at']}

    @contextmanager
    def save_lock(self):
        require(fcntl is not None, 'Description saves require a POSIX file lock')
        # Share the content-research writer lock so the two savers cannot update minerals.db together.
        lock_dir = self.data / 'pilots/mineral-content-queue-v1'
        lock_dir.mkdir(parents=True, exist_ok=True)
        descriptor = os.open(lock_dir / 'save.lock', os.O_CREAT | os.O_RDWR, 0o600)
        try:
            fcntl.flock(descriptor, fcntl.LOCK_EX)
            yield
        finally:
            fcntl.flock(descriptor, fcntl.LOCK_UN)
            os.close(descriptor)

    def apply_description(self, connection, material, properties, known, record, now):
        text = record['text']
        urls = record['source_urls']
        research = properties.get('public_research')
        if not isinstance(research, dict):
            research = {}
            properties['public_research'] = research
        previous = research.get('description') if isinstance(research.get('description'), dict) else None
        previous_text = known['existing_description']
        if record['action'] == 'retain' and previous:
            research['description'] = previous
        else:
            if 'publication_status' not in research:
                research['publication_status'] = 'published'
            if research.get('review_status') not in ('reviewed', 'verified'):
                research['review_status'] = 'reviewed'
            description = dict(previous or {})
            description.update({
                'text': text,
                'source_urls': urls,
                'license_spdx': 'CC-BY-4.0',
                'license_scope': 'Applies to this catalog summary only. Cited sources keep their own terms.',
                'production_kind': 'record_summary',
                'changes_notice': 'Original catalog summary written from facts already stored on the record.',
                'no_endorsement_notice': 'A cited source is not endorsing this summary.',
                'publication_status': research.get('publication_status'),
                'review_status': research.get('review_status'),
            })
            research['description'] = description
        if previous_text and previous_text != text:
            drafts = properties.get('research_draft_descriptions')
            if not isinstance(drafts, list):
                drafts = []
            drafts.append({
                'text': previous_text,
                'private_research': True,
                'publication_status': 'private',
                'review_status': 'draft',
                'production_kind': 'replaced_record_summary',
                'replaced_at': now,
            })
            properties['research_draft_descriptions'] = drafts
        search = material['search_text'] or ''
        if text not in search:
            search = (search + ' ' + text).strip()
        connection.execute(
            'UPDATE materials SET description=?, properties_json=?, search_text=?, updated_at=? WHERE id=?',
            (text, encoded(properties), search, now, material['id']))
        saved = connection.execute('SELECT description, properties_json FROM materials WHERE id=?',
                                   (material['id'],)).fetchone()
        require(saved['description'] == text, 'Saved description did not match the review')
        saved_properties = json.loads(saved['properties_json'])
        saved_text = saved_properties.get('public_research', {}).get('description', {}).get('text')
        require(saved_text == text or (record['action'] == 'retain' and previous and saved_text == previous.get('text')),
                'Saved research description did not match the review')

    def reopen_for_rewrite(self, public_id):
        require(isinstance(public_id, str) and public_id.startswith('mat_'), 'Rewrite needs a material public id')
        with self.transaction():
            row = self.db.execute('SELECT * FROM jobs WHERE public_id=?', (public_id,)).fetchone()
            require(row is not None, 'Mineral is not in the description queue')
            require(row['status'] in ('complete', 'pending'), 'Only a finished or waiting description can be reopened')
            self.db.execute(
                "UPDATE jobs SET status='pending', priority_group='rewrite', worker=NULL, token=NULL, "
                'claimed_at=NULL, lease_until=NULL, error=NULL WHERE public_id=?',
                (public_id,))
            self.event(row, 'reopen', 'rewrite')
        return {'result': 'reopened', 'public_id': public_id, 'priority_group': 'rewrite'}

    def complete(self, worker, token, review_file):
        path = self.private_file(review_file)
        record, raw = read_json(path)
        with self.save_lock():
            with self.transaction():
                row = self.db.execute('SELECT * FROM jobs WHERE worker=? AND token=?', (worker, token)).fetchone()
                require(row is not None, 'Unknown token or wrong worker')
                require(row['status'] == 'in_progress', 'Job is not in progress')
                require(record.get('material_public_id') == row['public_id'], 'Review mineral identity mismatch')
            now = stamp()
            with self.minerals(writable=True) as connection:
                connection.execute('BEGIN IMMEDIATE')
                try:
                    checkpoint = self.shared_checkpoint(now)
                    material, properties, facts, evidence = self.load_material(connection, row['public_id'])
                    require(material['canonical_name'] == row['canonical_name'], 'Live mineral name changed')
                    known = self.known_facts(material, properties, facts, evidence)
                    known['public_id_check'] = row['public_id']
                    if row['priority_group'] == 'rewrite':
                        known['action'] = 'compose'
                        known['priority_group'] = 'rewrite'
                    require(known['priority_group'] == row['priority_group'], 'Record priority changed; reconcile the claim')
                    self.validate_review(record, known)
                    cod_before = properties.get('public_research', {}).get('cod_records') if isinstance(
                        properties.get('public_research'), dict) else None
                    self.apply_description(connection, material, properties, known, record, now)
                    cod_after = json.loads(connection.execute(
                        'SELECT properties_json FROM materials WHERE id=?', (material['id'],)).fetchone()[0])
                    cod_saved = cod_after.get('public_research', {}).get('cod_records') if isinstance(
                        cod_after.get('public_research'), dict) else None
                    require(cod_saved == cod_before, 'COD records changed while saving a description')
                    receipt = {
                        'status': 'completed', 'committed_at': stamp(), 'public_id': row['public_id'],
                        'canonical_name': row['canonical_name'], 'action': record['action'],
                        'worker': worker, 'claim_token': token, 'review_sha256': sha(raw),
                        'text_sha256': sha(record['text'].encode()), 'source_urls': record['source_urls'],
                        **checkpoint,
                    }
                    connection.commit()
                except BaseException:
                    connection.rollback()
                    raise
            receipt_path = self.work / 'receipts' / (row['public_id'] + '-' + token + '.json')
            receipt_path.write_text(encoded(receipt), encoding='utf-8')
            digest = file_hash(receipt_path)
            with self.transaction():
                current = self.db.execute('SELECT * FROM jobs WHERE public_id=?', (row['public_id'],)).fetchone()
                require(current and current['status'] == 'in_progress' and current['token'] == token,
                        'Queue assignment changed while saving the description')
                self.db.execute(
                    "UPDATE jobs SET status='complete', receipt_path=?, receipt_sha256=?, completed_at=?, error=NULL "
                    'WHERE public_id=?', (str(receipt_path), digest, now, row['public_id']))
                self.event(current, 'complete', record['action'])
            return {'result': 'complete', 'public_id': row['public_id'], 'action': record['action'],
                    'receipt_path': str(receipt_path)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command', choices=('next', 'status', 'complete', 'reopen'))
    parser.add_argument('--worker')
    parser.add_argument('--token')
    parser.add_argument('--review-file')
    parser.add_argument('--public-id')
    parser.add_argument('--priority-group', choices=GROUPS)
    parser.add_argument('--root', type=Path, default=ROOT)
    parser.add_argument('--data', type=Path, default=DATA)
    args = parser.parse_args()
    require(Path('/.dockerenv').exists(), 'Run inside the existing admin container through tools/container-task.sh')
    queue = DescriptionQueue(args.root, args.data)
    try:
        if args.command == 'status':
            result = queue.status()
        elif args.command == 'next':
            require(args.worker, 'next requires --worker')
            result = queue.next(args.worker, args.priority_group)
        elif args.command == 'reopen':
            require(args.public_id, 'reopen requires --public-id')
            result = queue.reopen_for_rewrite(args.public_id)
        else:
            require(args.worker and args.token and args.review_file, 'complete requires --worker, --token and --review-file')
            result = queue.complete(args.worker, args.token, args.review_file)
        print(encoded(result))
    finally:
        queue.close()


if __name__ == '__main__':
    main()
