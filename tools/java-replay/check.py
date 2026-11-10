#!/usr/bin/env python3
"""Read-only assertions against the real developer-control state, not a replay model."""
import argparse
import json
import os
import socket
import time
from pathlib import Path


def rpc(endpoint, command):
    settings = json.loads(Path(endpoint).read_text())
    with socket.create_connection(('127.0.0.1', settings['port']), timeout=120) as stream:
        stream.sendall(json.dumps(dict(command, id=time.monotonic_ns(), token=settings['token'])).encode() + b'\n')
        line = stream.makefile('rb').readline(32 * 1024 * 1024)
        reply = json.loads(line)
        if not reply.get('ok'):
            raise ValueError(reply.get('error', 'developer command failed'))
        return reply['result']


def pointer(value, path):
    for part in path.strip('/').split('/'):
        part = part.replace('~1', '/').replace('~0', '~')
        value = value[int(part)] if isinstance(value, list) else value[part]
    return value


def evaluate(state, expectations):
    results = []
    for row in expectations:
        actual = pointer(state, row['path'])
        operation = row.get('op', 'eq')
        if operation == 'eq':
            passed = actual == row['value']
        elif operation == 'min':
            passed = isinstance(actual, (int, float)) and actual >= row['value']
        elif operation == 'contains':
            passed = row['value'] in actual
        else:
            raise ValueError('unsupported fixture assertion: ' + operation)
        results.append(dict(path=row['path'], op=operation, expected=row['value'], actual=actual, passed=passed))
    return results


def private_json(path, value):
    with os.fdopen(os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), 'w') as out:
        json.dump(value, out, indent=2, allow_nan=False)
        out.write('\n')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--endpoint', required=True)
    parser.add_argument('--expect', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--screenshot', type=Path)
    args = parser.parse_args()
    state = rpc(args.endpoint, {'cmd': 'state'})
    results = evaluate(state, json.loads(args.expect.read_text()))
    # Deliberately retain only explicitly selected fields; never copy the RPC token/whole profile.
    private_json(args.out, {'assertions': results, 'passed': all(row['passed'] for row in results)})
    if args.screenshot:
        rpc(args.endpoint, {'cmd': 'screenshot', 'path': str(args.screenshot.resolve())})
    print(f"State assertions: {sum(row['passed'] for row in results)}/{len(results)} passed")
    if not all(row['passed'] for row in results):
        raise SystemExit(1)


if __name__ == '__main__':
    main()
