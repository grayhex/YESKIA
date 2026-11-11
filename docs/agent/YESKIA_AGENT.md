# Opt-in YESKIA agent extension (#34)

The baseline ADBian daemon and USB PCCS stay independent. The optional **yeskiaagent**
helper is a one-shot headless SIS; it owns no Bluetooth listener, has no autostart and
adds no phone screen UI. It reads one versioned request file, calls read-only system
APIs, writes one response, then exits. It does not patch upstream rshelld.

## Implemented source, not a verified phone binary

`agent/yeskia-agent/` contains the Symbian entry and a narrow C++ AppArc bridge.
`crates/yeskia-agent-protocol` is a no_std/alloc typed protocol with fake-system tests.
The first additional service is **apps.registry.read**, unavailable in base ADBian:
RApaLsSession::GetAllApps/GetNextApp returns UID and caption, at most 32 records/page.
Caption truncation is explicit; the bridge has a fixed 32-row buffer, no global app
cache, no launch/remove API. Symbian Leaves are contained by TRAP on the C++ side.

The AppArc end-of-list is the positive `RApaLsSession::ENoMoreAppsInList`, not a negative
KErrNotFound. Positive EAppListInvalid is treated as failure. Source reference:
[public AppArc header](https://github.com/SymbianSource/oss.FCL.sf.mw.appsupport/blob/3efd2b6c5ad920873846770a70f9769721e494c8/appfw/apparchitecture/inc/APGCLI.H).
This API reference is not a substitute for compiling against the owner's FP2 SDK or
checking loader imports/capabilities on E72. No Nokia SDK header is vendored here.

## Build and invocation

Prepare the pinned ARM64 EPOC environment from EPOC_BUILD.md, then:

```sh
python3 scripts/agent/stage-helper.py /path/to/agent-build
# The script prints the source hash. Build verifies staged source and selects the pinned nightly.
python3 scripts/agent/build.py build-helper /path/to/agent-build
```

Staging copies only YESKIA-owned sources and creates an external device Cargo workspace
with relative dependencies on the pinned SDK. The device profile uses a 1 MiB heap,
ReadUserData/WriteUserData, DEBUG=0 and provisional development UID 0xE0AA00F3. Stock
installation/signing is not assumed. Use a registered UID and audit licensing before
distributing the helper. Record the produced SIS SHA-256 separately from the source hash.
The Rust device entry was checked for the real ARMv5TE custom target with the pinned nightly (`cargo check -Zbuild-std=core,alloc`). This checks Rust types/ABI declarations only. C++ compilation, SIS link/package, loader and runtime remain **not tested** without ARM64 + Nokia SDK + E72.

After installing on the phone, arm Lab and choose YESKIA hello/capabilities/App registry.
The confirmation lists the exact request-file write and `exec C:/sys/bin/yeskiaagent.exe`.
Host writes/read-back-verifies `C:/Data/_yeskia/request.json`, launches once, then polls
`response.json`. A random request ID ties the result to this invocation. Old responses,
wrong protocol versions/services, oversized documents and failed helper results cannot
establish success. A timeout never relaunches the helper automatically.

Protocol 1 uses a tagged `Query` (hello, capabilities, apps_list) inside typed JSON,
not invented text shell verbs. Responses include agent version, source build, request ID,
optional own-heap readings and result. The document cap is 32 KiB, page cap 32, scan cap
4096, caption cap 64 characters. Host tests encode 100 worst-case caption pages; their
timing is **not an E72 benchmark**. Measure before/after/peak heap, elapsed time, repeated
pages and denied API calls on the physical phone. No personal output goes into logs.

A returned app page can add `adbian-agent:apps.registry.read` evidence to system.read on
that connection. A declared service or base pong never marks PIM supported. A registry
listing is neither package installation evidence nor proof that any listed app launches.

## PIM audit and remaining implementation

| Domain | Confirmed in pinned upstream source | Required next implementation and test |
| --- | --- | --- |
| Contacts | No Contacts Model/Phonebook export shim or ADBian verb found | Isolated CContactDatabase/Contacts Model bridge (`cntmodel`), real UID/import/signing review against FP2 headers, bounded records/vCard, synthetic contacts only for first read |
| Calendar/tasks | `symbian::agenda` is EPOC's own reminder-file codec; it is **not** native Calendar database access. `cal` is a separate upstream app | Audit cal's store and native Agenda/Calendar APIs; isolate CCalSession/view bridge and async load; export JSON/iCalendar with recurrence/timezone tests |
| SMS | `symbian::msg` + shim_msg expose CMsvSession/MTM read APIs and some mutation APIs; base ADBian exposes none of these | Separate read-only SMS helper; avoid bringing all message imports into rshelld; folder/PDU/body/status tests, safe explicit local export |
| App registry | Safe API and new bridge source implemented | Compile SIS, validate E72 AppArc imports, page completeness/concurrency and memory |

All protocol-1 PIM capability flags are **false**. No PIM write/delete/restore/sync routes
are exposed. Those require read tests first, then separate confirmed diff/dry-run/read-back
workflows and hardware evidence. The agent PIM implementation remains outstanding; existing
native USB PCCS routes continue to provide their own separately tested functions.
