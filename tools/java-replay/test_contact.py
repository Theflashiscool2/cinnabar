"""Assertions used by the native contact capture, including the original missing gesture."""
import unittest
from run_contact import check_contact_order
class ContactOrder(unittest.TestCase):
    def test_adventure_gesture_does_not_grant_break_permission(self):
        check_contact_order('adventure', [['ServerboundPunchPacket']])
        with self.assertRaises(AssertionError):
            check_contact_order('adventure', [['ServerboundPlayerActionPacket', 0], ['ServerboundPunchPacket']])
    def test_original_missing_gesture_fails(self):
        with self.assertRaises(AssertionError):
            check_contact_order('adventure', [])
    def test_survival_start_precedes_punch(self):
        check_contact_order('survival', [['ServerboundPlayerActionPacket', 0], ['ServerboundPunchPacket']])
        with self.assertRaises(AssertionError):
            check_contact_order('survival', [['ServerboundPunchPacket'], ['ServerboundPlayerActionPacket', 0]])
