"""Acceptance-runner parsing/timing checks; no client/server or movement simulation."""
import unittest
from run_flight import parse_abilities, parse_position, toggle_timing


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


if __name__ == '__main__':
    unittest.main()
