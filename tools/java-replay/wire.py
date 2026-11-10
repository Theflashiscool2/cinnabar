"""Bounded framing for local fixtures; packet IDs come from the pinned codec."""
import hashlib
import ipaddress
import json
import re
import socket
import struct
import uuid
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TARGET = json.loads((ROOT / 'crates/java-protocol/data/target.json').read_text())
CODEC = (ROOT / 'crates/java-protocol/data/MinecraftCodec.java').read_text()
MAX_FRAME = 8 * 1024 * 1024
FIXTURE_NAME = 'CinnabarTest'
FIXTURE_ID = uuid.UUID(bytes=hashlib.md5(('OfflinePlayer:' + FIXTURE_NAME).encode()).digest(), version=3)


def tables(direction):
    """Use the same ordered registrations as the native codec generator."""
    return {phase: re.findall(r'\.register' + direction + r'Packet\((\w+)\.class',
            CODEC.split('.state(ProtocolState.' + state + ',', 1)[1].split('.state(ProtocolState.', 1)[0])
            for phase, state in [('login', 'LOGIN'), ('configuration', 'CONFIGURATION'), ('play', 'GAME')]}


CLIENTBOUND, SERVERBOUND = tables('Clientbound'), tables('Serverbound')


def vi(value):
    if not 0 <= value <= 0x7fffffff:
        raise ValueError('fixture VarInt out of range')
    out = bytearray()
    while value > 127:
        out.append((value & 127) | 128)
        value >>= 7
    out.append(value)
    return bytes(out)


def read_vi(data, offset=0):
    value = 0
    for index in range(5):
        if offset >= len(data):
            raise ValueError('truncated VarInt')
        byte = data[offset]
        offset += 1
        value |= (byte & 127) << (7 * index)
        if not byte & 128:
            if value > 0x7fffffff:
                raise ValueError('fixture negative/overflow VarInt')
            return value, offset
    raise ValueError('oversized VarInt')


def string(value):
    data = value.encode()
    return vi(len(data)) + data


def read_string(data, offset=0):
    size, offset = read_vi(data, offset)
    if size > 32767 or offset + size > len(data):
        raise ValueError('invalid fixture string')
    return data[offset:offset + size].decode(), offset + size


def exact(stream, count):
    out = bytearray()
    while len(out) < count:
        part = stream.recv(count - len(out))
        if not part:
            raise EOFError('fixture connection closed')
        out.extend(part)
    return bytes(out)


def receive(stream, compression=None):
    prefix = bytearray()
    for _ in range(5):
        prefix.extend(exact(stream, 1))
        if not prefix[-1] & 128:
            break
    size, _ = read_vi(prefix)
    if not 0 < size <= MAX_FRAME:
        raise ValueError('fixture frame limit')
    raw = exact(stream, size)
    body = raw
    if compression is not None:
        expanded, start = read_vi(raw)
        if expanded:
            if expanded > MAX_FRAME:
                raise ValueError('fixture expansion limit')
            decoder = zlib.decompressobj()
            body = decoder.decompress(raw[start:], expanded + 1)
            if len(body) != expanded or not decoder.eof or decoder.unused_data or decoder.unconsumed_tail:
                raise ValueError('invalid fixture compression')
        else:
            body = raw[start:]
    if not body:
        raise ValueError('empty fixture packet')
    return bytes(prefix) + raw, body


def frame(body, compression=None):
    if not 0 < len(body) <= MAX_FRAME:
        raise ValueError('fixture frame limit')
    if compression is not None:
        body = vi(len(body)) + zlib.compress(body) if len(body) >= compression else b'\0' + body
    return vi(len(body)) + body


def name(body, phase, server=True):
    packet, _ = read_vi(body)
    table = (CLIENTBOUND if server else SERVERBOUND)[phase]
    if packet >= len(table):
        raise ValueError('unknown fixture packet ID')
    return table[packet]


def loopback(address):
    host, port = address.rsplit(':', 1)
    if not ipaddress.ip_address(host).is_loopback or not 0 < int(port) <= 65535:
        raise ValueError('fixture requires numeric loopback address and valid port')
    return host, int(port)


def login(stream):
    """Consume a pinned-protocol offline fixture handshake; never accept real profiles."""
    _, body = receive(stream)
    packet, pos = read_vi(body)
    protocol, pos = read_vi(body, pos)
    _, pos = read_string(body, pos)
    pos += 2
    intent, pos = read_vi(body, pos)
    if packet != 0 or protocol != TARGET['protocol'] or intent not in (2, 3) or pos != len(body):
        raise ValueError('expected pinned fixture login handshake')
    handshake = body
    _, body = receive(stream)
    packet, pos = read_vi(body)
    profile, pos = read_string(body, pos)
    if packet != 0 or profile != FIXTURE_NAME or body[pos:] != FIXTURE_ID.bytes:
        raise ValueError('only the named offline fixture profile is accepted')
    return handshake, body


def validate_finish(body):
    packet, pos = read_vi(body)
    identity = body[pos:pos + 16]
    profile, pos = read_string(body, pos + 16)
    properties, pos = read_vi(body, pos)
    if (packet != CLIENTBOUND['login'].index('ClientboundLoginFinishedPacket')
            or identity != FIXTURE_ID.bytes or profile != FIXTURE_NAME
            or properties != 0 or len(body) != pos + 16):
        raise ValueError('server did not return the offline fixture identity')


def transfer(body, port):
    """Retarget only an already-loopback server transfer to the owned fixture listener."""
    packet, pos = read_vi(body)
    host, pos = read_string(body, pos)
    old_port, pos = read_vi(body, pos)
    loopback(f'{host}:{old_port}')
    if pos != len(body):
        raise ValueError('trailing transfer fields')
    return vi(packet) + string('127.0.0.1') + vi(port)
