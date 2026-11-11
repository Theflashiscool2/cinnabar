"""Acceptance-runner parsing/timing checks; no client/server or movement simulation."""
import unittest
from run_flight import parse_abilities, parse_position, toggle_timing, StableTerminal, full_terminal, scalar_receipt


class FlightEvidenceTests(unittest.TestCase):
    def test_native_snbt_receipts(self):
        self.assertEqual(parse_abilities('Fixture has the following entity data: {flying: 0b, mayfly: 1b, flySpeed: 0.05f}'),
                         {'flying': False, 'mayfly': True})
        self.assertEqual(parse_position('Fixture has the following entity data: [114.5d, 69.0d, -8.5d]'),
                         [114.5, 69., -8.5])
        with self.assertRaises(ValueError):
            parse_abilities('Unknown entity')
        with self.assertRaises(ValueError):
            parse_position('[1.0d, 2.0d]')

    def test_native_window_is_not_widened_by_rpc_latency(self):
        self.assertEqual(toggle_timing({'before': 10, 'after': 11}, {'before': 13, 'after': 14})['verdict'], 'within-window')
        self.assertEqual(toggle_timing({'before': 10, 'after': 11}, {'before': 16, 'after': 17})['verdict'], 'within-window')
        self.assertEqual(toggle_timing({'before': 10, 'after': 11}, {'before': 16, 'after': 18})['verdict'], 'inconclusive')
        self.assertEqual(toggle_timing({'before': 10, 'after': 12}, {'before': 11, 'after': 14})['verdict'], 'inconclusive')


class FlightCaptureTests(unittest.TestCase):
    def capture_fixture(self, path):
        from unittest.mock import MagicMock, patch
        from flight_capture import FlightCapture
        from tape import encoded
        from wire import CLIENTBOUND, FIXTURE_ID, FIXTURE_NAME, string, vi
        listener = MagicMock()
        listener.accept.side_effect = [(MagicMock(), None), (MagicMock(), None)]
        login_finish = vi(CLIENTBOUND['login'].index('ClientboundLoginFinishedPacket')) + FIXTURE_ID.bytes + string(FIXTURE_NAME) + vi(0) + bytes(16)

        def protocol_capture(client, target, writer, segment, port, server_opened):
            server_opened(MagicMock())
            writer.append({'type': 'segment', 'segment': segment, 'login_finish': encoded(login_finish)})
            writer.packet(segment, 'server', 'configuration', b'\x00')

        with patch('flight_capture.socket.create_server', return_value=listener), patch('flight_capture.capture', side_effect=protocol_capture):
            recorder = FlightCapture(path, '127.0.0.1:25587', '127.0.0.1:25586')
            recorder.worker.join(timeout=2)
        self.assertFalse(recorder.worker.is_alive())
        self.assertEqual(listener.accept.call_count, 2)
        return recorder

    def test_exactly_two_connections_complete_only_after_successful_driver(self):
        import tempfile
        from pathlib import Path
        from tape import load
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'session.jsonl'
            recorder = self.capture_fixture(path)
            self.assertTrue(recorder.finish(True)['complete'])
            self.assertEqual(len(load(path)), 2)

    def test_failed_driver_retires_even_two_segments_without_complete_trailer(self):
        import tempfile
        from pathlib import Path
        from tape import load
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'session.jsonl'
            recorder = self.capture_fixture(path)
            self.assertFalse(recorder.finish(False)['complete'])
            with self.assertRaisesRegex(ValueError, 'incomplete'):
                load(path)


class TerminalBaselineTests(unittest.TestCase):
    def snapshot(self, tick=0):
        return {'native_java': {'phase': 'Play', 'columns': 49,
                'inventory': {'storage': [], 'opened': None, 'selected': 0},
                'entities': {'count': 0}, 'vitals': {'health': 20., 'food': 20}},
                'native_java_hud': {'alive': True, 'health': [20, 20, 1],
                                    'food': [20, 20, 1], 'inventory_ui_error': None},
                'native_java_render': {'loading_stage': 'ready'}, 'sidebar': None,
                'native_java_movement': {'ticks': tick, 'on_ground': True, 'blocked': None}}

    def test_ready_spawn_column_and_rounded_health_do_not_close_terminal(self):
        snapshot = self.snapshot()
        self.assertTrue(full_terminal(snapshot, 49))
        snapshot['native_java']['columns'] = 1
        self.assertFalse(full_terminal(snapshot, 49))
        snapshot['native_java']['columns'] = 49
        snapshot['native_java']['vitals']['health'] = 19.5
        self.assertFalse(full_terminal(snapshot, 49))
        self.assertEqual(scalar_receipt('Fixture has the following entity data: 20.0f'), 20.)

    def test_projection_must_remain_full_and_stable_for_twenty_ticks(self):
        stable = StableTerminal(49)
        self.assertFalse(stable.observe(self.snapshot(0), 20.))
        self.assertFalse(stable.observe(self.snapshot(19), 20.))
        changed = self.snapshot(20)
        changed['native_java']['entities']['count'] = 1
        self.assertFalse(stable.observe(changed, 20.))
        changed['native_java_movement']['ticks'] = 40
        self.assertTrue(stable.observe(changed, 20.))
        self.assertFalse(stable.observe(changed, 19.))
        self.assertFalse(stable.observe(changed, 20.))


if __name__ == '__main__':
    unittest.main()
