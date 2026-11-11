#!/usr/bin/env python3
"""Capture/replay local vanilla fixture traffic through the real Cinnabar client."""
import argparse
import collections
import socket
import struct
import threading
import time
from pathlib import Path
from tape import Writer, encoded, load
from wire import (CLIENTBOUND, SERVERBOUND, MAX_FRAME, FIXTURE_NAME, frame, login, loopback,
                  name, read_vi, receive, transfer, validate_finish, vi, tcp_nodelay)


def advance(phase, packet):
    if packet in ('ClientboundLoginFinishedPacket', 'ServerboundLoginAcknowledgedPacket',
                  'ClientboundStartConfigurationPacket', 'ServerboundConfigurationAcknowledgedPacket'):
        return 'configuration'
    if packet in ('ClientboundFinishConfigurationPacket', 'ServerboundFinishConfigurationPacket'):
        return 'play'
    return phase


def stop(stream):
    try:
        stream.shutdown(socket.SHUT_RDWR)
    except OSError:
        pass
    stream.close()


def capture(client, target, writer, segment, port, server_opened=None):
    tcp_nodelay(client)
    handshake, hello = login(client)
    with socket.create_connection(target, timeout=10) as server:
        tcp_nodelay(server)
        if server_opened is not None:
            server_opened(server)
        server.settimeout(None)
        _capture(client, server, handshake, hello, writer, segment, port)


def _capture(client, server, handshake, hello, writer, segment, port):
    server.sendall(frame(handshake) + frame(hello))
    compression = None
    # Login is deliberately synchronous: no credentials, encryption or profile properties enter a tape.
    while True:
        raw, body = receive(server, compression)
        packet = name(body, 'login')
        if packet == 'ClientboundHelloPacket':
            stop(server)
            raise ValueError('online-mode/encryption is forbidden in fixture capture')
        if packet == 'ClientboundLoginCompressionPacket':
            _, pos = read_vi(body)
            threshold, end = read_vi(body, pos)
            if end != len(body) or threshold > MAX_FRAME:
                raise ValueError('invalid fixture compression threshold')
            client.sendall(raw)
            compression = threshold
        elif packet == 'ClientboundLoginFinishedPacket':
            validate_finish(body)
            writer.append({'type': 'segment', 'segment': segment, 'login_finish': encoded(body)})
            client.sendall(raw)
            raw, ack = receive(client, compression)
            if name(ack, 'login', False) != 'ServerboundLoginAcknowledgedPacket':
                raise ValueError('expected login acknowledgement')
            server.sendall(raw)
            break
        else:
            stop(server)
            raise ValueError('unexpected offline fixture login response: ' + packet)
    errors = []

    def pump(source, destination, direction):
        phase = 'configuration'
        try:
            while True:
                raw, body = receive(source, compression)
                packet = name(body, phase, direction == 'server')
                if packet == 'ClientboundTransferPacket':
                    body = transfer(body, port)
                    raw = frame(body, compression)
                writer.packet(segment, direction, phase, body)
                destination.sendall(raw)
                phase = advance(phase, packet)
        except (EOFError, ConnectionError):
            pass
        except Exception as error:
            errors.append(error)
        finally:
            stop(destination)

    worker = threading.Thread(target=pump, args=(client, server, 'client'))
    worker.start()
    pump(server, client, 'server')
    worker.join(timeout=10)
    if worker.is_alive():
        raise ValueError('capture worker did not stop')
    if errors:
        raise errors[0]


# Gates observe real automatic replies; no fake movement, transactions or gameplay acknowledgements.
GATES = {'ServerboundFinishConfigurationPacket', 'ServerboundConfigurationAcknowledgedPacket',
         'ServerboundSelectKnownPacks', 'ServerboundAcceptTeleportationPacket', 'ServerboundKeepAlivePacket',
         'ServerboundInteractPacket', 'ServerboundContainerClickPacket', 'ServerboundContainerClosePacket',
         'ServerboundUseItemPacket', 'ServerboundUseItemOnPacket', 'ServerboundPlayerActionPacket',
         'ServerboundSwingPacket', 'ServerboundSetCarriedItemPacket'}


# Pinned 26.3 ServerCommonPacketListenerImpl.keepConnectionAlive: 15000 ms.
SERVER_KEEPALIVE_SECONDS = 15
MAX_PENDING_GATES = 4096
MAX_PENDING_GATE_BYTES = 2 * MAX_FRAME


def hold(finished, heartbeat):
    """Preserve inspection state while servicing genuine fixture-server keepalive requests."""
    while not finished.wait(SERVER_KEEPALIVE_SECONDS):
        heartbeat()


def replay(client, segment, port, speed):
    tcp_nodelay(client)
    login(client)
    client.sendall(frame(segment['login_finish']))
    _, ack = receive(client)
    if name(ack, 'login', False) != 'ServerboundLoginAcknowledgedPacket':
        raise ValueError('expected replay login acknowledgement')
    observed = collections.Counter()
    pending = [0, 0]
    heartbeat_key = [None]
    condition = threading.Condition()
    finished = threading.Event()
    errors = []

    def collect():
        phase = 'configuration'
        try:
            while True:
                _, body = receive(client)
                packet = name(body, phase, False)
                if packet in GATES:
                    with condition:
                        key = (phase, body)
                        if key == heartbeat_key[0]:
                            heartbeat_key[0] = None
                        else:
                            if pending[0] >= MAX_PENDING_GATES or pending[1] + len(body) > MAX_PENDING_GATE_BYTES:
                                raise ValueError('fixture pending response limit')
                            observed[key] += 1
                            pending[0] += 1
                            pending[1] += len(body)
                        condition.notify_all()
                phase = advance(phase, packet)
        except (EOFError, ConnectionError):
            pass
        except Exception as error:
            errors.append(error)
        finally:
            finished.set()
            with condition:
                condition.notify_all()

    worker = threading.Thread(target=collect)
    worker.start()
    packets = segment['packets']
    origin = packets[0]['ns'] if packets else 0
    start = time.monotonic()
    sent = 0
    try:
        for row in packets:
            body = row['body']
            packet = name(body, row['phase'], row['direction'] == 'server')
            if row['direction'] == 'client':
                if packet in GATES:
                    key = row['phase'], body
                    with condition:
                        if not condition.wait_for(lambda: observed[key] or finished.is_set(), timeout=30):
                            raise ValueError('missing native replay response: ' + packet)
                        if not observed[key]:
                            raise ValueError('client ended before replay response: ' + packet)
                        observed[key] -= 1
                        pending[0] -= 1
                        pending[1] -= len(body)
                        if not observed[key]:
                            del observed[key]
                continue
            remaining = (row['ns'] - origin) / 1e9 / speed - (time.monotonic() - start)
            if remaining > 0 and finished.wait(remaining):
                raise ValueError('client ended before replay completed')
            if packet == 'ClientboundTransferPacket':
                body = transfer(body, port)
            client.sendall(frame(body))
            sent += 1
        print(f'Replay segment completed: server_packets={sent}; holding for state assertions/disconnect', flush=True)
        final_phase = 'configuration'
        if packets:
            last = packets[-1]
            final_phase = advance(last['phase'], name(last['body'], last['phase'], last['direction'] == 'server'))

        def heartbeat():
            with condition:
                if heartbeat_key[0] is not None:
                    raise ValueError('native client did not answer fixture keepalive')
                identity = struct.pack('>q', time.monotonic_ns())
                reply = vi(SERVERBOUND[final_phase].index('ServerboundKeepAlivePacket')) + identity
                heartbeat_key[0] = (final_phase, reply)
            client.sendall(frame(vi(CLIENTBOUND[final_phase].index('ClientboundKeepAlivePacket')) + identity))

        hold(finished, heartbeat)
    finally:
        stop(client)
        worker.join(timeout=10)
    if errors:
        raise errors[0]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=['capture', 'replay', 'inspect'])
    parser.add_argument('--tape', type=Path, required=True)
    parser.add_argument('--listen', default='127.0.0.1:25587')
    parser.add_argument('--target', default='127.0.0.1:25586')
    parser.add_argument('--sessions', type=int, default=1)
    parser.add_argument('--speed', type=float, default=1)
    args = parser.parse_args()
    if not 0 < args.sessions <= 32 or not 0 < args.speed <= 10:
        parser.error('sessions 1..32 and speed >0..10 required')
    address, target = loopback(args.listen), loopback(args.target)
    segments = load(args.tape) if args.mode != 'capture' else None
    if args.mode == 'inspect':
        print({'protocol_segments': len(segments), 'packets': [len(s['packets']) for s in segments],
               'privacy': 'fixture text/world data; not a public diagnostic summary'})
        return
    writer = Writer(args.tape) if args.mode == 'capture' else None
    complete = False
    try:
        with socket.socket() as listener:
            listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            listener.bind(address)
            listener.listen(4)
            print(f'{args.mode} listening on {address[0]}:{address[1]}; offline profile {FIXTURE_NAME}', flush=True)
            for index in range(args.sessions if writer else len(segments)):
                client, _ = listener.accept()
                try:
                    if writer:
                        capture(client, target, writer, index, address[1])
                    else:
                        replay(client, segments[index], address[1], args.speed)
                finally:
                    stop(client)
            complete = True
    finally:
        if writer:
            if complete:
                writer.close()
            else:
                writer.file.close()  # No completion footer: interrupted tapes must fail closed.


if __name__ == '__main__':
    main()
