#!/usr/bin/env python3
"""Authored private-loopback flight acceptance against an already running native client/server.

Uses native gamemode/teleport authority, persistent raw key holds and explicit releases.
Never launches processes, changes allow-flight, or widens the native seven-tick window.
Screenshots require human inspection; this does not establish public-server acceptance.
"""
import argparse
import hashlib
import json
import re
import time
from pathlib import Path
from check import private_json, rpc
from harness import rcon
from flight_capture import FlightCapture
from run_fixture import until
from wire import FIXTURE_NAME, loopback


def parse_abilities(text):
    values = dict(re.findall(r'\b(mayfly|flying|invulnerable|instabuild): ([01])b', text))
    if not {'mayfly', 'flying'} <= values.keys():
        raise ValueError('native abilities response missing flags: ' + text)
    return {key: value == '1' for key, value in values.items()}


def parse_position(text):
    match = re.search(r'\[([^\[\]]+)\]', text)
    if not match:
        raise ValueError('native position response missing vector: ' + text)
    values = [float(value.strip().removesuffix('d')) for value in match[1].split(',')]
    if len(values) != 3:
        raise ValueError('native position is not a three-vector')
    return values


def toggle_timing(first, second):
    """Bounds enclose input sampling; RPC receipt alone is never a sampled-edge claim."""
    minimum = second['before'] + 1 - first['after']
    maximum = second['after'] - first['before'] - 1
    return {'minimum_tick_gap': minimum, 'maximum_tick_gap': maximum,
            'verdict': 'within-window' if 2 <= minimum <= maximum <= 6 else 'inconclusive'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--endpoint', required=True, type=Path)
    parser.add_argument('--fixture', required=True, type=Path)
    parser.add_argument('--out', required=True, type=Path, help='new private evidence directory')
    parser.add_argument('--capture', action='store_true', help='capture exactly initial connection and reconnect')
    parser.add_argument('--listen', default='127.0.0.1:25587', help='private loopback capture listener')
    args = parser.parse_args()
    config = json.loads(args.fixture.read_text())
    address = f"127.0.0.1:{config['port']}"
    loopback(address)
    properties = dict(line.split('=', 1) for line in (args.fixture.parent / 'server.properties').read_text().splitlines()
                      if '=' in line and not line.startswith('#'))
    if properties.get('online-mode') != 'false' or properties.get('server-ip') != '127.0.0.1':
        raise ValueError('requires harness offline loopback server properties')
    output = args.out.resolve()
    output.mkdir(parents=True, mode=0o700, exist_ok=False)
    command = lambda value: rpc(args.endpoint, value)
    state = lambda: command({'cmd': 'state'})
    console = lambda text: rcon(config, text)
    if state().get('native_java') is not None:
        raise ValueError('client must start at its menu')
    events, checks = [], []
    connected, complete = False, False
    recorder, capture_result = None, None
    sources = [Path(__file__), *[Path(__file__).with_name(name) for name in
               ['check.py', 'harness.py', 'wire.py', 'run_fixture.py', 'session.py', 'tape.py', 'flight_capture.py']]]
    source_bytes = {path.name: path.read_bytes() for path in sources}
    private_json(output / 'source-hashes.json', {name: hashlib.sha256(data).hexdigest() for name, data in source_bytes.items()})
    (output / 'driver-source.py').write_bytes(source_bytes[Path(__file__).name])

    def sample(label):
        snapshot = state()
        raw = console(f'data get entity {FIXTURE_NAME} abilities')
        position = console(f'data get entity {FIXTURE_NAME} Pos')
        row = {'label': label, 'monotonic_ns': time.monotonic_ns(),
               'client_tick': snapshot['native_java_movement']['ticks'],
               'server_abilities_raw': raw, 'server_abilities': parse_abilities(raw),
               'server_position_raw': position, 'server_position': parse_position(position),
               'server_tick_raw': console('time query gametime'), 'client': snapshot}
        events.append(row)
        return row

    def ticks():
        snapshot = state()
        movement = snapshot.get('native_java_movement') or {}
        if movement.get('blocked'):
            raise ValueError('movement blocked: ' + str(movement['blocked']))
        return movement['ticks']

    def advance(count):
        before = ticks()
        return until(ticks, lambda tick: tick >= before + count, 'native movement ticks', timeout=15)

    def key(name, held, count=1):
        before = ticks()
        receipt = command({'cmd': 'input', 'hold' if held else 'release': [name]})
        after = until(ticks, lambda tick: tick >= before + count, 'raw-key observation interval', timeout=15)
        row = {'key': name, 'held': held, 'before': before, 'after': after,
               'receipt': receipt, 'monotonic_ns': time.monotonic_ns()}
        events.append({'input': row})
        return row

    def server_flight(expected):
        return until(lambda: parse_abilities(console(f'data get entity {FIXTURE_NAME} abilities')),
                     lambda abilities: abilities['flying'] == expected, 'native server flying flag', timeout=10)

    def check(label, passed):
        checks.append({'label': label, 'passed': bool(passed)})
        if not passed:
            raise ValueError('failed: ' + label)

    def picture(label):
        command({'cmd': 'screenshot', 'path': str(output / (label + '.png'))})

    try:
        if args.capture:
            recorder = FlightCapture(output / 'session.jsonl', args.listen, address)
            address = args.listen
        command({'cmd': 'connect_java_fixture', 'address': address})
        connected = True
        until(state, lambda s: (s.get('native_java_render') or {}).get('loading_stage') == 'ready', 'spawn renderer')
        command({'cmd': 'input', 'release_all': True})
        for text in [f'gamemode survival {FIXTURE_NAME}',
                     'fill 110 68 110 118 68 118 minecraft:stone',
                     'fill 110 69 110 118 90 118 minecraft:air',
                     f'tp {FIXTURE_NAME} 114.5 69 114.5 0 0',
                     f'gamemode creative {FIXTURE_NAME}']:
            events.append({'console': text, 'response': console(text)})
        advance(4)
        initial = sample('native-mayfly-grant')
        check('creative native grant is mayfly-only', initial['server_abilities']['mayfly'] and not initial['server_abilities']['flying'])
        first = key('key.jump', True)
        key('key.jump', False)
        second = key('key.jump', True)
        key('key.jump', False)
        timing = toggle_timing(first, second)
        events.append({'double_jump_timing': timing, 'note': 'input sample bounds, not packet-write receipts'})
        advance(2)
        toggled = sample('manual-double-jump')
        if timing['verdict'] == 'within-window':
            check('in-window manual double Jump reaches server', toggled['server_abilities']['flying'])
        else:
            checks.append({'label': 'manual double Jump', 'passed': None, 'status': 'inconclusive', 'timing': timing})
        # Remaining cases use a native spectator grant, independent of a mistimed double tap.
        console(f'gamemode spectator {FIXTURE_NAME}')
        server_flight(True)
        advance(2)
        spectator = sample('native-spectator-grant')
        check('spectator active flight stays ungrounded', not spectator['client']['native_java_movement']['on_ground'])
        picture('spectator')
        console(f'gamemode creative {FIXTURE_NAME}')
        server_flight(True)
        before = sample('before-rise')
        key('key.jump', True, 8)
        key('key.jump', False)
        raised = sample('rise')
        check('raw held Jump rises', raised['server_position'][1] > before['server_position'][1])
        picture('rise')
        key('key.sneak', True, 4)
        key('key.sneak', False)
        descended = sample('descend')
        check('raw held sneak descends', descended['server_position'][1] < raised['server_position'][1])
        revision = descended['client']['native_java'].get('teleport')
        console(f'tp {FIXTURE_NAME} 114.5 77 114.5 0 0')
        until(state, lambda s: s['native_java'].get('teleport') != revision, 'admitted correction')
        advance(3)
        corrected = sample('correction-during-flight')
        check('native correction preserves granted flying', corrected['server_abilities']['flying'])
        picture('correction')
        console(f'gamemode survival {FIXTURE_NAME}')
        server_flight(False)
        advance(3)
        revoked = sample('native-survival-revocation')
        check('native revocation removes mayfly', not revoked['server_abilities']['mayfly'])
        until(state, lambda s: s['native_java_movement']['on_ground'], 'survival landing', timeout=20)
        console(f'gamemode spectator {FIXTURE_NAME}')
        server_flight(True)
        console(f'tp {FIXTURE_NAME} 114.5 71 114.5 0 0')
        advance(3)
        console(f'gamemode creative {FIXTURE_NAME}')
        server_flight(True)
        key('key.sneak', True)
        until(state, lambda s: s['native_java_movement']['on_ground'], 'legal creative landing', timeout=20)
        key('key.sneak', False)
        server_flight(False)
        landed = sample('native-auto-land')
        check('landing retains native mayfly', landed['server_abilities']['mayfly'])
        picture('landing')
        command({'cmd': 'disconnect'})
        until(state, lambda s: s.get('native_java') is None, 'disconnect retirement')
        command({'cmd': 'connect_java_fixture', 'address': address})
        until(state, lambda s: (s.get('native_java_render') or {}).get('loading_stage') == 'ready', 'reconnected renderer')
        advance(3)
        reconnected = sample('reconnected-native-authority')
        check('reconnect receives native creative permission', reconnected['server_abilities']['mayfly'])
        picture('reconnect')
        complete = True
    finally:
        cleanup_errors = []
        if connected:
            try:
                command({'cmd': 'input', 'release_all': True})
                command({'cmd': 'disconnect'})
                until(state, lambda s: s.get('native_java') is None, 'final connection retirement')
            except Exception as error:
                cleanup_errors.append(str(error))
        if recorder is not None:
            try:
                capture_result = recorder.finish(complete and not cleanup_errors)
            except Exception as error:
                capture_result = {'complete': False, 'error': str(error)}
        private_json(output / 'flight-evidence.json', {'checks': checks, 'events': events,
                     'scope': 'authored private fixture; inspect screenshots; no public-server acceptance',
                     'capture': capture_result, 'cleanup_errors': cleanup_errors,
                     'complete': complete and not cleanup_errors and (capture_result is None or capture_result['complete'])})
    if cleanup_errors or (capture_result is not None and not capture_result['complete']):
        raise ValueError('flight run/capture did not retire completely; inspect evidence')
    inconclusive = any(row.get('status') == 'inconclusive' for row in checks)
    print('INCONCLUSIVE: double-Jump timing; inspect private evidence.' if inconclusive else 'PASS: authored flight authority checks; inspect screenshots.')
    return 2 if inconclusive else 0


if __name__ == '__main__':
    raise SystemExit(main())
