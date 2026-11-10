"""Read-only adapter contract tests; no phone or python-gammu required."""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("reader", Path(__file__).resolve().parents[1] / "crates/yeskia-core/src/personal/reader.py")
r = importlib.util.module_from_spec(spec)
spec.loader.exec_module(r)

class ReaderTests(unittest.TestCase):
    def test_verified_zero_sms_is_empty(self):
        class Phone:
            def GetSMSStatus(self): return {"PhoneUsed": 0}
        self.assertEqual(r.read_records(Phone(), "sms"), [])

    def test_missing_sms_status_is_not_empty_success(self):
        class Phone:
            def GetSMSStatus(self): return {}
        with self.assertRaises(r.PhoneOperationError): r.read_records(Phone(), "sms")

    def test_partial_contacts_are_rejected(self):
        class ERR_EMPTY(Exception): pass
        class Phone:
            def GetMemoryStatus(self, **kw): return {"Used": 2}
            def GetNextMemory(self, **kw):
                if "Start" in kw: return {"Location": 1, "Entries": []}
                raise ERR_EMPTY()
        with self.assertRaises(ERR_EMPTY): r.read_records(Phone(), "contacts")

    def test_iteration_must_advance(self):
        class Phone:
            def GetCalendarStatus(self): return {"Used": 2}
            def GetNextCalendar(self, **kw): return {"Location": 1}
        with self.assertRaises(r.PhoneOperationError): r.read_records(Phone(), "calendar")

    def test_contact_fields_and_sms_parts(self):
        contact = r.public({"Location": 1, "Entries": [{"Type": "Text_Name", "Value": "Test"}, {"Type": "Number_Mobile", "Value": "123"}]}, "contacts")
        self.assertEqual(contact["phones"], ["123"])
        sms = r.public({"Location": 1, "Folder": 1, "Type": "Deliver", "UDH": {"PartNumber": 2, "AllParts": 3}}, "sms")
        self.assertEqual((sms["direction"], sms["part"], sms["parts"]), ("incoming", 2, 3))

if __name__ == "__main__": unittest.main()
