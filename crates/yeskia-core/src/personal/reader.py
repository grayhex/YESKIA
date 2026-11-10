# Read-only subset adapted from grayhex/s60modern efaf53d, gammu_native.py and personal.py.
# Python 3 + python-gammu. Data is returned privately, never printed as diagnostics.
import json
import re
import sys
from datetime import datetime
MAX_RECORDS = 5000
DOMAINS = {
 "contacts": ("GetNextMemory", {"Type": "ME"}, "GetMemoryStatus", {"Type": "ME"}),
 "calendar": ("GetNextCalendar", {}, "GetCalendarStatus", {}),
 "tasks": ("GetNextToDo", {}, "GetToDoStatus", {}),
 "sms": ("GetNextSMS", {"Folder": 0}, "GetSMSStatus", {}),
}
class PhoneOperationError(ValueError):
 pass
def error_code(exc) -> str:
    name = type(exc).__name__
    return name if re.fullmatch("ERR_[A-Z_]+", name) else "ERR_CONNECTION"


def read_records(sm, domain: str) -> list[dict]:
    method, args, status_method, status_args = DOMAINS[domain]
    # A successful explicit zero count proves an empty store. ERR_EMPTY alone
    # cannot distinguish an unsupported service from a genuinely empty store.
    status = getattr(sm, status_method)(**status_args)
    count = (
        status.get("Used")
        if domain != "sms"
        else sum(status.get(k, 0) for k in ("SIMUsed", "PhoneUsed", "TemplatesUsed"))
    )
    if domain == "sms" and not any(k in status for k in ("SIMUsed", "PhoneUsed", "TemplatesUsed")):
        raise PhoneOperationError("SMS count is unavailable")
    if count == 0:
        return []
    result, seen, location = [], set(), None
    while True:
        try:
            value = getattr(sm, method)(
                **args, **({"Start": True} if location is None else {"Location": location})
            )
        except Exception as exc:
            if (
                error_code(exc) == "ERR_EMPTY"
                and result
                and (count is None or len(result) >= count)
            ):
                return result
            raise
        records = value if isinstance(value, list) else [value]
        if not records:
            raise PhoneOperationError("Empty iteration response")
        last = records[-1].get("Location")
        if last is None or last in seen or len(result) + len(records) > MAX_RECORDS:
            raise PhoneOperationError(
                "Phone data exceeds the safe read limit or iteration did not advance"
            )
        seen.add(last)
        result.extend(records)
        location = last


def fields(record: dict) -> dict:
    return {entry["Type"]: entry.get("Value") for entry in record.get("Entries", [])}


def text(value) -> str:
    return value.isoformat() if isinstance(value, datetime) else str(value or "")


def public(record: dict, domain: str) -> dict:
    f = fields(record)
    identifier = f"{domain}:{record.get('MemoryType', '')}:{record.get('Location')}"
    if domain == "contacts":
        name = (
            f.get("Text_Name")
            or " ".join(str(f.get(k) or "") for k in ("Text_FirstName", "Text_LastName")).strip()
        )
        return {
            "native_id": identifier,
            "name": name,
            "given": f.get("Text_FirstName", ""),
            "family": f.get("Text_LastName", ""),
            "phones": [
                str(e["Value"])
                for e in record.get("Entries", [])
                if e["Type"].startswith("Number_")
            ],
            "emails": [
                str(e["Value"])
                for e in record.get("Entries", [])
                if e["Type"] in ("Text_Email", "Text_Email2")
            ],
        }
    if domain == "sms":
        return {
            "native_id": identifier + f":{record.get('Folder', 0)}",
            "number": str(record.get("Number") or ""),
            "text": str(record.get("Text") or ""),
            "date": text(record.get("DateTime")),
            "state": record.get("State") or "unknown",
            "folder": record.get("Folder"),
            "direction": "incoming"
            if record.get("Type") == "Deliver"
            else "outgoing"
            if record.get("Type") == "Submit"
            else "unknown",
            "part": (record.get("UDH") or {}).get("PartNumber"),
            "parts": (record.get("UDH") or {}).get("AllParts"),
        }
    return {
        "native_id": identifier,
        "uid": str(f.get("LUID") or identifier),
        "kind": domain,
        "summary": str(f.get("TEXT") or ""),
        "description": str(f.get("DESCRIPTION") or ""),
        "location": str(f.get("LOCATION") or ""),
        "start": text(f.get("START_DATETIME")),
        "end": text(f.get("END_DATETIME")) if domain == "calendar" else "",
        "due": text(f.get("END_DATETIME")) if domain == "tasks" else "",
        "completed": bool(f["COMPLETED"]) if "COMPLETED" in f else None,
        "priority": record.get("Priority", "None") if domain == "tasks" else None,
        "recurrence": any(k.startswith("REPEAT_") for k in f),
    }



def main():
 sm = None
 try:
  import gammu
  request = json.loads(sys.stdin.read(65536))
  domain = request["domain"]
  if domain not in DOMAINS:
   raise ValueError()
  gammu.SetDebugLevel("nothing")
  sm = gammu.StateMachine()
  sm.ReadConfig(Filename=request["config"])
  sm.SetDebugLevel("nothing")
  sm.Init()
  rows = [public(r, domain) for r in read_records(sm, domain)]
  payload = json.dumps(rows, ensure_ascii=True)
  if len(payload) > 8 * 1024 * 1024:
   raise ValueError()
 except Exception:
  sys.exit(1)
 finally:
  if sm is not None:
   try:
    sm.Terminate()
   except Exception:
    pass
 sys.stdout.write(payload)
if __name__ == "__main__":
 main()
