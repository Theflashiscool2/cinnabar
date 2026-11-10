#!/usr/bin/env python3
"""Compare explicit native-state projections and bounded functional telemetry observations."""
import argparse
import json
from pathlib import Path
from check import evaluate, pointer, private_json

# Wall times and player-simulation coordinates are deliberately not deterministic replay assertions.
PATHS = ['/native_java/phase', '/native_java/columns', '/native_java/inventory/storage',
         '/native_java/inventory/opened', '/native_java/inventory/selected', '/sidebar',
         '/native_java_hud/alive', '/native_java_hud/health', '/native_java_hud/food',
         '/native_java_hud/inventory_ui_error', '/native_java/entities/count']
ANOMALIES = ['spawn_over_existing_entity_id', 'despawns_without_current_identity',
             'container_input_stale', 'container_update_admission_refused',
             'container_click_not_written', 'remote_spawn_using_local_player_id',
             'teleport_admission_refused', 'world_events_not_admitted',
             'action_command_queue_full', 'command_queue_closed', 'active_animation_lost_hand_publication']


def compare(baseline, replay):
    assertions = evaluate(replay, [{'path': path, 'value': pointer(baseline, path)} for path in PATHS])
    counts = {}
    # Report all functional counters, including expected differences from omitted client inputs.
    live = baseline.get('native_java_telemetry')
    played = replay.get('native_java_telemetry')
    if live and played:
        for key in live['counts'].keys() | played['counts'].keys():
            before = live['counts'].get(key, {}).get('count', 0)
            after = played['counts'].get(key, {}).get('count', 0)
            counts[key] = {'capture': before, 'replay': after, 'delta': after - before}
        # Treat only named anomaly increases as regressions, not fewer actions as successful gameplay.
        for key in ANOMALIES:
            if key not in counts:
                raise ValueError('telemetry schema lacks required counter: ' + key)
            if key in counts:
                assertions.append({'path': 'telemetry/' + key, 'passed': counts[key]['delta'] <= 0,
                                   'actual': counts[key], 'expected': 'no increase'})
    else:
        assertions.append({'path': 'telemetry', 'passed': False, 'expected': 'enabled in capture and replay'})
    return {'passed': all(row['passed'] for row in assertions), 'assertions': assertions,
            'functional_counters': counts,
            'limitations': ['Inbound state-admission replay, not authoritative acceptance of replayed actions.',
                            'No frame-time equality, skin-fetch fidelity, online/iOS/Hypixel claim.',
                            'World coverage: phase, loaded column count and entity count; not per-block equality.']}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('baseline', type=Path)
    parser.add_argument('replay', type=Path)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    result = compare(json.loads(args.baseline.read_text()), json.loads(args.replay.read_text()))
    private_json(args.out, result)
    print(f"Replay comparison: {sum(row['passed'] for row in result['assertions'])}/{len(result['assertions'])} passed")
    if not result['passed']:
        raise SystemExit(1)


if __name__ == '__main__':
    main()
