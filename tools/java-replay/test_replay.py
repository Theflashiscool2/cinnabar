"""Focused fixture integrity, transport and replay sequencing checks (no game simulation)."""
import json
import copy
from compare import ANOMALIES, compare
from check import evaluate
import socket
import tempfile
import threading
import unittest
import zlib
from pathlib import Path
from unittest.mock import patch
import session
from tape import Writer, encoded, load
from wire import (CLIENTBOUND, SERVERBOUND, TARGET, FIXTURE_ID, FIXTURE_NAME, MAX_FRAME,
                  frame, login, loopback, name, read_vi, receive, string, transfer, vi)


def finish():
    return vi(CLIENTBOUND['login'].index('ClientboundLoginFinishedPacket')) + FIXTURE_ID.bytes + string(FIXTURE_NAME) + vi(0) + bytes(16)


def connect_bytes(profile=FIXTURE_NAME):
    return (frame(vi(0) + vi(TARGET['protocol']) + string('127.0.0.1') + b'\x63\xdd' + vi(2))
            + frame(vi(0) + string(profile) + FIXTURE_ID.bytes))


class WireTests(unittest.TestCase):
    def test_compression_round_trip_and_reject_trailing_stream(self):
        for compression in (None, 0, 64):
            left, right = socket.socketpair()
            with left, right:
                original = b'\x01' + b'x' * 200
                left.sendall(frame(original, compression))
                self.assertEqual(receive(right, compression)[1], original)
        raw = vi(2) + zlib.compress(b'ab') + b'junk'
        left, right = socket.socketpair()
        with left, right:
            left.sendall(vi(len(raw)) + raw)
            with self.assertRaises(ValueError):
                receive(right, 0)

    def test_bounds_and_malformed_varint(self):
        for bad in (b'', b'\x80', b'\xff' * 5, b'\xff\xff\xff\xff\x0f'):
            with self.assertRaises(ValueError):
                read_vi(bad)
        left, right = socket.socketpair()
        with left, right:
            left.sendall(vi(MAX_FRAME + 1))
            with self.assertRaises(ValueError):
                receive(right)

    def test_offline_identity_and_numeric_loopback_required(self):
        self.assertEqual(loopback('127.0.0.1:25587'), ('127.0.0.1', 25587))
        for address in ('hypixel.net:25565', '1.1.1.1:25565', '127.0.0.1:0'):
            with self.assertRaises(ValueError):
                loopback(address)
        for profile, succeeds in ((FIXTURE_NAME, True), ('RealAccount', False)):
            left, right = socket.socketpair()
            with left, right:
                left.sendall(connect_bytes(profile))
                if succeeds:
                    self.assertEqual(len(login(right)), 2)
                else:
                    with self.assertRaises(ValueError):
                        login(right)

    def test_transfer_is_retargeted_only_from_loopback(self):
        body = vi(7) + string('127.0.0.1') + vi(25586)
        self.assertEqual(transfer(body, 25587), vi(7) + string('127.0.0.1') + vi(25587))
        with self.assertRaises(ValueError):
            transfer(vi(7) + string('mc.hypixel.net') + vi(25565), 25587)

    def test_online_encryption_request_is_not_captured(self):
        client, relay_client = socket.socketpair()
        server, relay_server = socket.socketpair()
        with client, relay_client, server, relay_server:
            client.sendall(connect_bytes())
            server.sendall(frame(vi(CLIENTBOUND['login'].index('ClientboundHelloPacket'))))
            with patch('session.socket.create_connection', return_value=relay_server):
                with self.assertRaisesRegex(ValueError, 'online-mode/encryption'):
                    session.capture(relay_client, ('127.0.0.1', 1), None, 0, 25587)


class TapeTests(unittest.TestCase):
    def make_tape(self, path):
        writer = Writer(path)
        writer.append({'type': 'segment', 'segment': 0, 'login_finish': encoded(finish())})
        writer.packet(0, 'server', 'configuration', b'\x00')
        writer.close()

    def test_complete_checksum_and_no_overwrite(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'fixture.jsonl'
            self.make_tape(path)
            self.assertEqual(len(load(path)), 1)
            self.assertEqual(path.stat().st_mode & 0o777, 0o600)
            with self.assertRaises(FileExistsError):
                Writer(path)
            data = path.read_bytes()
            path.write_bytes(data.replace(b'configuration', b'configuratxon'))
            with self.assertRaisesRegex(ValueError, 'checksum'):
                load(path)
            path.write_bytes(data[:data.rfind(b'{"type": "complete"')])
            with self.assertRaises(ValueError):
                load(path)

    def test_native_replay_gates_and_final_world_hold(self):
        finish_config = vi(CLIENTBOUND['configuration'].index('ClientboundFinishConfigurationPacket'))
        ack = vi(SERVERBOUND['configuration'].index('ServerboundFinishConfigurationPacket'))
        play = vi(CLIENTBOUND['play'].index('ClientboundSetHeldSlotPacket')) + b'\x02'
        segment = {'login_finish': finish(), 'packets': [
            {'direction': 'server', 'phase': 'configuration', 'body': finish_config, 'ns': 0},
            {'direction': 'client', 'phase': 'configuration', 'body': ack, 'ns': 0},
            {'direction': 'server', 'phase': 'play', 'body': play, 'ns': 0}]}
        client, replay = socket.socketpair()
        errors = []
        def run():
            try:
                session.replay(replay, segment, 25587, 1)
            except Exception as error:
                errors.append(error)
        worker = threading.Thread(target=run)
        worker.start()
        with client:
            client.sendall(connect_bytes())
            self.assertEqual(receive(client)[1], finish())
            client.sendall(frame(vi(SERVERBOUND['login'].index('ServerboundLoginAcknowledgedPacket'))))
            self.assertEqual(receive(client)[1], finish_config)
            client.sendall(frame(ack))
            self.assertEqual(receive(client)[1], play)
            self.assertTrue(worker.is_alive())
        worker.join(timeout=5)
        self.assertFalse(worker.is_alive())
        self.assertEqual(errors, [])

    def test_missing_native_gameplay_request_fails_replay(self):
        interaction = vi(SERVERBOUND['play'].index('ServerboundInteractPacket')) + vi(1) + vi(1) + b'\0'
        finish_config = vi(CLIENTBOUND['configuration'].index('ClientboundFinishConfigurationPacket'))
        ack = vi(SERVERBOUND['configuration'].index('ServerboundFinishConfigurationPacket'))
        segment = {'login_finish': finish(), 'packets': [
            {'direction': 'server', 'phase': 'configuration', 'body': finish_config, 'ns': 0},
            {'direction': 'client', 'phase': 'configuration', 'body': ack, 'ns': 0},
            {'direction': 'client', 'phase': 'play', 'body': interaction, 'ns': 0}]}
        client, replay = socket.socketpair()
        errors = []
        def run():
            try:
                session.replay(replay, segment, 25587, 1)
            except ValueError as error:
                errors.append(str(error))
        worker = threading.Thread(target=run)
        worker.start()
        with client:
            client.sendall(connect_bytes())
            receive(client)
            client.sendall(frame(vi(SERVERBOUND['login'].index('ServerboundLoginAcknowledgedPacket'))))
            receive(client)
            client.sendall(frame(ack))
        worker.join(timeout=5)
        self.assertFalse(worker.is_alive())
        self.assertEqual(errors, ['client ended before replay response: ServerboundInteractPacket'])

    def test_final_inspection_hold_services_heartbeat_until_disconnect(self):
        class Finished:
            def __init__(self):
                self.calls = 0
            def wait(self, interval):
                self.calls += 1
                return self.calls == 3
        done = Finished()
        beats = []
        session.hold(done, lambda: beats.append('real server request'))
        self.assertEqual(beats, ['real server request', 'real server request'])

    def test_inspection_heartbeat_uses_native_request_and_reply_frames(self):
        finish_config = vi(CLIENTBOUND['configuration'].index('ClientboundFinishConfigurationPacket'))
        ack = vi(SERVERBOUND['configuration'].index('ServerboundFinishConfigurationPacket'))
        segment = {'login_finish': finish(), 'packets': [
            {'direction': 'server', 'phase': 'configuration', 'body': finish_config, 'ns': 0},
            {'direction': 'client', 'phase': 'configuration', 'body': ack, 'ns': 0}]}
        client, replay = socket.socketpair()
        errors = []
        def inspection(finished, heartbeat):
            heartbeat()
            finished.wait()
        def run():
            try:
                session.replay(replay, segment, 25587, 1)
            except Exception as error:
                errors.append(error)
        with patch('session.hold', inspection):
            worker = threading.Thread(target=run)
            worker.start()
            with client:
                client.sendall(connect_bytes())
                receive(client)
                client.sendall(frame(vi(SERVERBOUND['login'].index('ServerboundLoginAcknowledgedPacket'))))
                receive(client)
                client.sendall(frame(ack))
                _, request = receive(client)
                self.assertEqual(name(request, 'play'), 'ClientboundKeepAlivePacket')
                _, payload = read_vi(request)
                self.assertEqual(len(request[payload:]), 8)
                client.sendall(frame(vi(SERVERBOUND['play'].index('ServerboundKeepAlivePacket')) + request[payload:]))
            worker.join(timeout=5)
        self.assertFalse(worker.is_alive())
        self.assertEqual(errors, [])


class StateComparisonTests(unittest.TestCase):
    def state(self):
        return {'native_java': {'phase': 'Play', 'columns': 49,
                'inventory': {'storage': [{'Known': {'item': 'minecraft:white_wool', 'count': 20}}],
                              'opened': None, 'selected': 0}, 'entities': {'count': 2}},
                'sidebar': {'rows': [{'text': 'Fixture', 'score': 11}]},
                'native_java_hud': {'alive': True, 'health': [20, 20, 1], 'food': [20, 20, 1],
                                    'inventory_ui_error': None},
                'native_java_telemetry': {'counts': {key: {'count': 0} for key in ANOMALIES}}}

    def test_wrong_inventory_world_hud_and_anomalies_fail(self):
        original = self.state()
        self.assertTrue(compare(original, copy.deepcopy(original))['passed'])
        for field, changed in [('columns', 0), ('phase', 'Configuration')]:
            replay = copy.deepcopy(original)
            replay['native_java'][field] = changed
            self.assertFalse(compare(original, replay)['passed'])
        replay = copy.deepcopy(original)
        replay['native_java']['inventory']['storage'][0]['Known']['count'] = 16
        replay['sidebar']['rows'][0]['score'] = 7
        replay['native_java_telemetry']['counts'][ANOMALIES[0]]['count'] = 1
        self.assertFalse(compare(original, replay)['passed'])

    def test_disabled_or_changed_telemetry_schema_is_not_a_clean_comparison(self):
        original = self.state()
        replay = copy.deepcopy(original)
        replay['native_java_telemetry'] = None
        self.assertFalse(compare(original, replay)['passed'])
        replay = copy.deepcopy(original)
        del replay['native_java_telemetry']['counts'][ANOMALIES[0]]
        del original['native_java_telemetry']['counts'][ANOMALIES[0]]
        with self.assertRaises(ValueError):
            compare(original, replay)

    def test_explicit_state_assertions_fail_on_wrong_or_missing_facts(self):
        self.assertTrue(evaluate({'a': [7]}, [{'path': '/a/0', 'value': 7}])[0]['passed'])
        self.assertFalse(evaluate({'a': [7]}, [{'path': '/a/0', 'value': 8}])[0]['passed'])
        with self.assertRaises(KeyError):
            evaluate({}, [{'path': '/a/0', 'value': 7}])


if __name__ == '__main__':
    unittest.main()
