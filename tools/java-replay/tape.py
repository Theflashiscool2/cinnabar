"""Private, bounded, checksummed fixture transcripts. Never publish captures."""
import base64
import hashlib
import json
import os
import threading
import time
from wire import TARGET, CODEC, MAX_FRAME, FIXTURE_NAME, FIXTURE_ID, validate_finish

MAX_TAPE = 256 * 1024 * 1024
MAX_EVENTS = 100000
MAX_NS = 60 * 60 * 1_000_000_000


def encoded(body):
    return base64.b64encode(body).decode('ascii')


def decoded(value):
    body = base64.b64decode(value, validate=True)
    if not 0 < len(body) <= MAX_FRAME:
        raise ValueError('fixture payload limit')
    return body


class Writer:
    def __init__(self, path):
        self.file = os.fdopen(os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), 'wb')
        self.lock = threading.Lock()
        self.digest = hashlib.sha256()
        self.bytes = self.events = 0
        self.started = time.monotonic_ns()
        self.append({'type': 'header', 'schema': 1, 'protocol': TARGET['protocol'],
                     'game_version': TARGET['game_version'], 'fixture_name': FIXTURE_NAME,
                     'fixture_uuid': str(FIXTURE_ID),
                     'codec_sha256': hashlib.sha256(CODEC.encode()).hexdigest(),
                     'privacy': 'LOCAL OFFLINE FIXTURE ONLY; may contain fixture text/world/identities'})

    def append(self, record):
        with self.lock:
            line = json.dumps(record, separators=(',', ':'), allow_nan=False).encode() + b'\n'
            if self.bytes + len(line) > MAX_TAPE or self.events >= MAX_EVENTS:
                raise ValueError('fixture transcript limit')
            self.file.write(line)
            self.digest.update(line)
            self.bytes += len(line)
            self.events += 1

    def packet(self, segment, direction, phase, body):
        self.append({'type': 'packet', 'segment': segment, 'direction': direction, 'phase': phase,
                     'ns': time.monotonic_ns() - self.started, 'body': encoded(body)})

    def close(self):
        with self.lock:
            line = json.dumps({'type': 'complete', 'events': self.events,
                               'sha256': self.digest.hexdigest()}).encode() + b'\n'
            self.file.write(line)
            self.file.flush()
            os.fsync(self.file.fileno())
            self.file.close()


def load(path):
    if os.path.getsize(path) > MAX_TAPE + 1024:
        raise ValueError('fixture transcript limit')
    digest, records = hashlib.sha256(), []
    with open(path, 'rb') as source:
        for line in source:
            row = json.loads(line)
            if row.get('type') == 'complete':
                if row.get('sha256') != digest.hexdigest() or row.get('events') != len(records) or source.read(1):
                    raise ValueError('fixture checksum/count/trailing data')
                break
            digest.update(line)
            records.append(row)
            if len(records) > MAX_EVENTS:
                raise ValueError('fixture event limit')
        else:
            raise ValueError('incomplete fixture transcript')
    header = records[0]
    if (header.get('schema') != 1 or header.get('protocol') != TARGET['protocol']
            or header.get('game_version') != TARGET['game_version']
            or header.get('fixture_name') != FIXTURE_NAME or header.get('fixture_uuid') != str(FIXTURE_ID)
            or header.get('codec_sha256') != hashlib.sha256(CODEC.encode()).hexdigest()):
        raise ValueError('fixture target/codec/identity mismatch')
    segments = {}
    for row in records[1:]:
        kind, segment = row.get('type'), row.get('segment')
        if kind == 'segment':
            if segment != len(segments):
                raise ValueError('nonsequential fixture segment')
            validate_finish(decoded(row['login_finish']))
            segments[segment] = {'login_finish': decoded(row['login_finish']), 'packets': []}
        elif kind == 'packet':
            if (segment not in segments or row.get('phase') not in ('configuration', 'play')
                    or row.get('direction') not in ('client', 'server') or not isinstance(row.get('ns'), int)
                    or not 0 <= row['ns'] <= MAX_NS):
                raise ValueError('invalid fixture packet record')
            segments[segment]['packets'].append(dict(row, body=decoded(row['body'])))
        else:
            raise ValueError('unexpected fixture record')
    if not segments:
        raise ValueError('empty fixture capture')
    return list(segments.values())
