#!/usr/bin/env python3
"""Controlled graphical shop/transfer capture, then native state-admission replay.

Requires the isolated harness server and a 1024x576 developer-control client already running
at its menu with offline fixture mode and telemetry enabled. No public-server automation.
"""
import argparse
import hashlib
import json
import re
import subprocess
import sys
import threading
import time
from pathlib import Path
from check import private_json, rpc
from compare import compare
from harness import rcon
from wire import FIXTURE_NAME, ROOT, loopback


def until(read, predicate, description, timeout=45):
    deadline = time.monotonic() + timeout
    while True:
        result = read()
        if predicate(result):
            return result
        if time.monotonic() >= deadline:
            raise ValueError('fixture timeout: ' + description)
        threading.Event().wait(0.05)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--endpoint', required=True, type=Path)
    parser.add_argument('--fixture', required=True, type=Path, help='private isolated server config')
    parser.add_argument('--out', required=True, type=Path, help='new private evidence directory')
    parser.add_argument('--port', default=25587, type=int)
    args = parser.parse_args()
    address = f'127.0.0.1:{args.port}'
    loopback(address)
    config = json.loads(args.fixture.read_text())
    output = args.out.resolve()
    output.mkdir(mode=0o700, parents=True, exist_ok=False)
    state = lambda: rpc(args.endpoint, {'cmd': 'state'})
    command = lambda value: rpc(args.endpoint, value)
    if state()['native_java'] is not None:
        raise ValueError('client must start at its menu; existing sessions are not retired by this runner')
    properties = dict(line.split('=', 1) for line in (args.fixture.parent / 'server.properties').read_text().splitlines()
                      if '=' in line and not line.startswith('#'))
    columns = (2 * int(properties['view-distance']) + 1) ** 2
    tape = output / 'session.jsonl'
    runner = Path(__file__).with_name('session.py')
    children, files = [], []

    def start(mode):
        log = output / f'{mode}.log'
        handle = log.open('x')
        log.chmod(0o600)
        files.append(handle)
        arguments = [sys.executable, str(runner), mode, '--tape', str(tape), '--listen', address]
        if mode == 'capture':
            arguments += ['--target', f"127.0.0.1:{config['port']}", '--sessions', '2']
        else:
            arguments += ['--speed', '10']
        child = subprocess.Popen(arguments, stdout=handle, stderr=subprocess.STDOUT)
        children.append(child)
        until(lambda: log.read_text(), lambda text: 'listening on' in text or child.poll() is not None,
              mode + ' listener')
        if child.poll() is not None:
            raise ValueError('fixture listener failed; inspect ' + str(log))
        return child, log

    def inventory_ok(snapshot):
        try:
            storage = snapshot['native_java']['inventory']['storage']
            return storage[0]['Known']['count'] == 7 and storage[2]['Known']['count'] == 20
        except (KeyError, TypeError):
            return False

    def ready(snapshot):
        return snapshot['native_java_render']['loading_stage'] == 'ready'

    def shop_input():
        def selected_shop(snapshot):
            try:
                selected = snapshot['native_java_interaction']['selected_entity']['network_id']
                return any(e['network_id'] == selected and e['kind'] == 'minecraft:villager'
                           for e in snapshot['native_java']['entities']['sample'])
            except (KeyError, TypeError):
                return False
        until(state, selected_shop, 'current real NPC selection')
        command({'cmd': 'input', 'look': {'yaw': 0, 'pitch': 0}, 'press': ['MouseRight'], 'press_frames': 1})
        until(state, lambda s: 'Container' in s['screens'] and
              s['native_java']['inventory']['opened']['state'] is not None, 'shop contents and screen')

    def click_and_close():
        command({'cmd': 'input', 'cursor': [512, 219], 'press': ['MouseLeft'], 'press_frames': 1})
        command({'cmd': 'input', 'release_all': True})
        until(state, inventory_ok, 'authoritative shop inventory response')
        command({'cmd': 'input', 'press': ['Escape'], 'press_frames': 1})
        command({'cmd': 'input', 'release_all': True})

    try:
        capture, _ = start('capture')
        command({'cmd': 'connect_java_fixture', 'address': address})
        before = until(state, ready, 'live spawn renderer')
        if before.get('native_java_telemetry') is None:
            raise ValueError('enable CINNABAR_JAVA_TELEMETRY=1 before starting this client')
        setup_receipts = []
        for text in [f'tp {FIXTURE_NAME} 104.5 69 104.5 0 0', f'clear {FIXTURE_NAME}',
                     f'give {FIXTURE_NAME} minecraft:iron_ingot 8',
                     f'give {FIXTURE_NAME} minecraft:shears', f'give {FIXTURE_NAME} minecraft:white_wool 16',
                     'scoreboard players set Fixture fixture 7', 'acceptance_shop']:
            setup_receipts.append(rcon(config, text))
        # Actual main-hand input reaches the real NPC's server-owned mobInteract method.
        until(state, lambda s: s['native_java_interaction']['selected_entity'] is not None,
              'live target after server setup')
        shop_input()
        command({'cmd': 'screenshot', 'path': str(output / 'live-shop.png')})
        click_and_close()
        receipt = rcon(config, f'data get entity {FIXTURE_NAME} Inventory')
        items = {int(slot): (name, int(count)) for count, slot, name in re.findall(
            r'\{count: (\d+), Slot: (\d+)b, id: "([^"]+)"\}', receipt)}
        if items.get(0) != ('minecraft:iron_ingot', 7) or items.get(2) != ('minecraft:white_wool', 20):
            raise ValueError('server inventory did not confirm purchase')
        epoch = state()['native_java_render']['epoch']
        rcon(config, 'scoreboard players set Fixture fixture 11')
        rcon(config, f"transfer 127.0.0.1 {config['port']} {FIXTURE_NAME}")
        live = until(state, lambda s: ready(s) and s['native_java_render']['epoch'] != epoch
                     and s['native_java']['columns'] == columns
                     and inventory_ok(s) and s.get('sidebar', {}).get('rows', [{}])[0].get('score') == 11,
                     'transferred world, sidebar and inventory')
        private_json(output / 'live-state.json', live)
        command({'cmd': 'screenshot', 'path': str(output / 'live-transfer.png')})
        command({'cmd': 'disconnect'})
        until(state, lambda s: s['native_java'] is None, 'live retirement')
        if capture.wait(timeout=15) != 0:
            raise ValueError('capture did not complete')
        playback, log = start('replay')
        command({'cmd': 'connect_java_fixture', 'address': address})
        until(state, lambda s: ready(s) and s['native_java']['inventory']['storage'][0] != 'Unknown'
              and s['native_java']['inventory']['storage'][0].get('Known', {}).get('count') == 8,
              'replayed setup before normal interaction')
        shop_input()
        command({'cmd': 'screenshot', 'path': str(output / 'replay-shop.png')})
        click_and_close()
        until(lambda: log.read_text(), lambda text: text.count('Replay segment completed:') == 2,
              'both replay segments', timeout=60)
        played = until(state, lambda s: ready(s) and inventory_ok(s) and s['native_java']['columns'] == columns, 'replayed native state')
        private_json(output / 'replay-state.json', played)
        command({'cmd': 'screenshot', 'path': str(output / 'replay-transfer.png')})
        result = compare(live, played)
        private_json(output / 'comparison.json', result)
        private_json(output / 'server-receipts.json', {'setup': setup_receipts, 'inventory': receipt})
        command({'cmd': 'disconnect'})
        until(state, lambda s: s['native_java'] is None, 'replay retirement')
        if playback.wait(timeout=15) != 0 or not result['passed']:
            raise ValueError('replay comparison failed; inspect private evidence')
        sources = [*runner.parent.glob('*.py'), *(ROOT / 'docs/testing/fixtures').glob('Vanilla*.java')]
        private_json(output / 'source-hashes.json', {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest()
                                                    for p in sources})
        print('PASS: server-confirmed pointer purchase, transfer, native replay state and anomaly comparison.')
        print('Inspect screenshots. No online/Hypixel/iPhone or replayed-action acceptance is established.')
    finally:
        for child in children:
            if child.poll() is None:
                command({'cmd': 'disconnect'})
                child.terminate()
                child.wait(timeout=10)
        for handle in files:
            handle.close()


if __name__ == '__main__':
    main()
