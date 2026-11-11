#!/usr/bin/env python3
"""Private D graphical checkpoint on the existing isolated fixture, or replay (no RCON).

The native peer must already be connected. This helper never launches a game/server,
connects clients, clears a world, awards inventory, or claims screenshots are visually correct.
"""
import argparse
import json
import re
import threading
import time
from pathlib import Path
from check import private_json, rpc
from harness import rcon
from run_fixture import until
from wire import FIXTURE_NAME


def receipt(config, viewer, peer, phase):
    text = rcon(config, f'acceptance_d {viewer} {peer} {phase}')
    try:
        return json.loads(text)
    except json.JSONDecodeError as error:
        raise ValueError('native D fixture did not return its JSON receipt: ' + text) from error


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--endpoint', required=True, type=Path)
    parser.add_argument('--fixture', type=Path)
    parser.add_argument('--peer', required=True)
    parser.add_argument('--viewer', default=FIXTURE_NAME)
    parser.add_argument('--phase', required=True, choices=['equip', 'bow', 'sneak', 'stop', 'block', 'swing',
                                                        'drops', 'pickup', 'snapshot', 'cleanup'])
    parser.add_argument('--out', required=True, type=Path, help='new private checkpoint directory')
    parser.add_argument('--replay', action='store_true', help='read/screenshot only; NEVER issue server fixture actions')
    args = parser.parse_args()
    if any(re.fullmatch(r'[A-Za-z0-9_]{1,16}', name) is None for name in [args.peer, args.viewer]):
        parser.error('explicit native player names required')
    if args.peer == args.viewer:
        parser.error('peer and viewer must differ')
    if not args.replay and not args.fixture:
        parser.error('--fixture required for live server receipts')
    output = args.out.resolve()
    output.mkdir(mode=0o700, parents=True, exist_ok=False)
    state = lambda: rpc(args.endpoint, {'cmd': 'state'})
    until(state, lambda s: (s.get('native_java_render') or {}).get('loading_stage') == 'ready', 'native renderer')
    config = json.loads(args.fixture.read_text()) if args.fixture else None
    before = None
    if not args.replay:
        before = receipt(config, args.viewer, args.peer, 'snapshot')
        action = receipt(config, args.viewer, args.peer, args.phase)
        private_json(output / 'server-action.json', action)
        if args.phase in ['bow', 'block'] and not action['using_item']:
            raise ValueError('native peer did not enter use; no pose acceptance claimed')
        if args.phase == 'drops':
            settled = until(lambda: receipt(config, args.viewer, args.peer, 'snapshot'),
                            lambda r: len(r['items']) == 2 and all(i['on_ground'] and i['count'] == 3 for i in r['items']),
                            'two native ordinary stacks settled on existing partial blocks')
            private_json(output / 'server-settled.json', settled)
    # Preserve successive exact client states: transient pickups are only 3 logical ticks.
    # RCON runs BEFORE polling; absence of a captured transient is inconclusive, never a pass.
    frames = []
    deadline = time.monotonic() + 2
    while True:
        frames.append({'elapsed': time.monotonic() - (deadline - 2), 'state': state()})
        if time.monotonic() >= deadline:
            break
        threading.Event().wait(0.025)
    private_json(output / 'client-frames.json', frames)
    rpc(args.endpoint, {'cmd': 'screenshot', 'path': str(output / 'frame.png')})
    if not args.replay:
        after = receipt(config, args.viewer, args.peer, 'snapshot')
        private_json(output / 'server-before.json', before)
        private_json(output / 'server-after.json', after)
        if args.phase == 'pickup' and after['viewer_iron'] - before['viewer_iron'] != 3:
            raise ValueError('expected exactly one ordinary 3-iron stack collected; inspect server receipts')
    print('Checkpoint saved; graphical correctness requires inspection: ' + str(output))


if __name__ == '__main__':
    main()
