#!/usr/bin/env python3
"""Isolated official vanilla server/world; never modifies an existing acceptance world."""
import argparse
import hashlib
import json
import os
import secrets
import socket
import struct
import subprocess
import threading
import time
import zipfile
from pathlib import Path
from wire import ROOT, TARGET, exact, loopback


def rcon(config, command):
    with socket.create_connection(('127.0.0.1', config['rcon_port']), timeout=10) as stream:
        def send(identity, kind, text):
            body = struct.pack('<ii', identity, kind) + text.encode() + b'\0\0'
            stream.sendall(struct.pack('<i', len(body)) + body)
        def read():
            length = struct.unpack('<i', exact(stream, 4))[0]
            if not 10 <= length <= 1024 * 1024:
                raise ValueError('invalid fixture RCON length')
            data = exact(stream, length)
            return struct.unpack('<ii', data[:8]), data[8:-2].decode()
        send(1, 3, config['rcon_password'])
        header, _ = read()
        if header[0] != 1:
            raise ValueError('fixture RCON authentication failed')
        send(2, 2, command)
        header, text = read()
        if header[0] != 2:
            raise ValueError('fixture RCON response mismatch')
        return text


def setup(config):
    rcon(config, 'forceload add 96 96 127 127')
    deadline = time.monotonic() + 30
    while 'There are' not in rcon(config, 'execute if loaded 104 68 104 run list'):
        if time.monotonic() >= deadline:
            raise ValueError('fixture columns did not load')
        threading.Event().wait(0.05)
    commands = [
        'gamerule minecraft:spawn_mobs false', 'gamerule minecraft:advance_weather false',
        'gamerule minecraft:advance_time false', 'time set day', 'weather clear',
        'fill 96 64 96 127 75 127 minecraft:air',
        'fill 96 68 96 127 68 127 minecraft:stone', 'setworldspawn 104 69 102',
        'setblock 100 69 110 minecraft:stone_slab[type=bottom]',
        'setblock 102 69 110 minecraft:stone_stairs[facing=north]',
        'setblock 104 69 110 minecraft:oak_fence',
        'setblock 106 69 110 minecraft:oak_trapdoor[half=bottom]',
        'fill 108 69 110 108 71 114 minecraft:stone',
        'fill 109 69 114 112 71 114 minecraft:stone',
        'setblock 114 68 110 minecraft:honey_block', 'setblock 116 69 110 minecraft:cobweb',
        'fill 118 68 109 120 68 111 minecraft:slime_block',
        'fill 122 68 109 124 68 111 minecraft:water',
        'setblock 110 69 106 minecraft:chest',
        'kill @e[tag=cinnabar_hologram]',
        'summon minecraft:armor_stand 104.5 71 107.5 {Tags:["cinnabar_hologram"],Invisible:1b,NoGravity:1b,CustomName:{text:"Fixture hologram"},CustomNameVisible:1b}',
        'acceptance_shop', 'scoreboard objectives add fixture dummy',
        'scoreboard objectives setdisplay sidebar fixture', 'scoreboard players set Fixture fixture 7',
        'tp @a 104.5 69 102.5 0 0', 'gamemode survival @a', 'clear @a',
    ]
    items = ['iron_ingot 8', 'white_wool 16', 'iron_sword', 'shears', 'fire_charge 4',
             'tnt 4', 'ender_pearl 4', 'bow', 'arrow 16', 'potion', 'stone 7']
    for index, item in enumerate(items):
        name, _, count = item.partition(' ')
        commands.append(f'item replace block 110 69 106 container.{index} with minecraft:{name} {count or 1}')
    commands.extend(['give @a minecraft:iron_ingot 8', 'give @a minecraft:shears',
                     'give @a minecraft:white_wool 16', 'setblock 104 69 105 minecraft:white_wool'])
    for command in commands:
        result = rcon(config, command)
        print(command.split(' ', 1)[0] + ': ' + result)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=['init', 'run', 'setup', 'command'])
    parser.add_argument('--root', required=True, type=Path)
    parser.add_argument('--server', type=Path)
    parser.add_argument('--libraries', type=Path)
    parser.add_argument('--java-home', type=Path)
    parser.add_argument('--port', type=int, default=25586)
    parser.add_argument('--rcon-port', type=int, default=25585)
    parser.add_argument('--accept-eula', action='store_true')
    parser.add_argument('--command')
    args = parser.parse_args()
    root = args.root.resolve()
    config_path = root / 'fixture-private.json'
    if args.mode == 'init':
        if not args.accept_eula:
            parser.error('read the Minecraft EULA, then provide --accept-eula')
        if not all([args.server, args.libraries, args.java_home]):
            parser.error('--server, --libraries and --java-home required')
        loopback(f'127.0.0.1:{args.port}')
        loopback(f'127.0.0.1:{args.rcon_port}')
        if args.port == args.rcon_port:
            parser.error('game and RCON ports must differ')
        server = args.server.resolve()
        if server.stat().st_size != TARGET['server_bytes'] or hashlib.sha256(server.read_bytes()).hexdigest() != TARGET['server_sha256']:
            raise ValueError('server archive does not match pinned official metadata')
        if root.exists():
            raise ValueError('refusing to initialize over an existing directory/world')
        root.mkdir(mode=0o700, parents=True)
        config = dict(server=str(server), libraries=str(args.libraries.resolve()),
                      java_home=str(args.java_home.resolve()), port=args.port, rcon_port=args.rcon_port,
                      rcon_password=secrets.token_hex(24))
        with os.fdopen(os.open(config_path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), 'w') as out:
            json.dump(config, out)
        properties = {'server-ip': '127.0.0.1', 'server-port': args.port,
                      'online-mode': 'false', 'enforce-secure-profile': 'false', 'accepts-transfers': 'true',
                      'enable-rcon': 'true', 'rcon.port': args.rcon_port, 'rcon.password': config['rcon_password'],
                      'level-name': 'controlled-world', 'level-type': 'minecraft:flat', 'generate-structures': 'false',
                      'spawn-protection': 0, 'view-distance': 3, 'simulation-distance': 3,
                      'max-players': 2, 'white-list': 'false', 'pause-when-empty-seconds': 0}
        with os.fdopen(os.open(root / 'server.properties', os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), 'w') as out:
            out.write(''.join(f'{key}={value}\n' for key, value in properties.items()))
        (root / 'eula.txt').write_text('eula=true\n')
        # Reuse the verified existing bundler extraction/cache; do not download replacement libraries.
        (root / 'libraries').symlink_to(args.libraries.resolve(), target_is_directory=True)
        print('Initialized isolated fixture; credentials remain in private config, not stdout.')
        return
    config = json.loads(config_path.read_text())
    if args.mode == 'setup':
        setup(config)
    elif args.mode == 'command':
        if not args.command:
            parser.error('--command required')
        print(rcon(config, args.command))
    else:
        java_home = Path(config['java_home'])
        fixtures = ROOT / 'docs/testing/fixtures'
        classes = root / 'fixture-classes'
        classes.mkdir(exist_ok=True)
        archive = Path(config['server'])
        if hashlib.sha256(archive.read_bytes()).hexdigest() != TARGET['server_sha256']:
            raise ValueError('pinned official server archive changed')
        jars = []
        with zipfile.ZipFile(archive) as bundled:
            for manifest, base in [('versions', Path(config['libraries']).parent / 'versions'),
                                   ('libraries', Path(config['libraries']))]:
                for row in bundled.read(f'META-INF/{manifest}.list').decode().splitlines():
                    digest, _, relative = row.split('\t')
                    jar = (base / relative).resolve()
                    if not jar.is_relative_to(base.resolve()) or hashlib.sha256(jar.read_bytes()).hexdigest() != digest:
                        raise ValueError('cached server class/library hash mismatch: ' + relative)
                    jars.append(jar)
        classpath = os.pathsep.join(map(str, jars))
        subprocess.run([str(java_home / 'bin/javac'), '-cp', classpath, '-d', str(classes),
                        *map(str, fixtures.glob('Vanilla*.java'))], check=True)
        os.chdir(root)
        os.execv(str(java_home / 'bin/java'), [str(java_home / 'bin/java'), '-Xms256M', '-Xmx768M',
                   '--add-opens', 'java.base/java.lang=ALL-UNNAMED', '-Dcinnabar.acceptance.shop=true',
                   '-cp', os.pathsep.join([str(classes), classpath]), 'VanillaDebugServer', '--nogui'])


if __name__ == '__main__':
    main()
