# E72 instrumentation research (#36)

These are **versioned experiment plans and synthetic evidence**, not three completed E72
hardware cases. Hardware observations are still required. Use the owner's laboratory E72
and record the exact model/RM/firmware/signing/capability state, YESKIA commit, rshelld SIS
hash, helper source/SIS hash, protocol version, host build and mode locally. In public
reports use an alias; keep MAC/IMEI/serials, ROM dumps, raw USBPcap/ETW and personal output
under ignored `agent-captures/` or outside the repository.

| Case | Test action and expected evidence | Failure / cleanup |
| --- | --- | --- |
| F01 filesystem | Lab: create `C:/Data/YESKIA-Test/`, upload a synthetic file, stat/read-back SHA-256, download a >1 MiB synthetic file from E: in ranges; interrupt one upload deliberately, inspect before manual retry | Never call an uncertain write successful. Verify both endpoint files; remove only the explicitly named test files through confirmed rm. No ROM/system mutation. |
| S01 system APIs | Read `hal`, a documented P&S key and explicit known CenRep key; run the optional helper once for capabilities and a bounded AppArc page; capture version, request ID, error codes and own-heap values | Denied/missing imports/caps are results. Keep app captions private. If helper cannot load, preserve baseline rshelld and isolate offending import rather than modifying system libraries. |
| T01 host/phone correlation | Record one native USB/PCCS read action, its Windows timestamp, optional owner's USBPcap/ETW capture, then a separate agent-side P&S/file/log observation. Repeat with Bluetooth interruption and prove USB still responds | Do not merge USB/agent identities from a model/name. No write replay. Restore the original connection preference after the case; verify independent routes. |

For each case distinguish planned action, actual host event, actual phone result, SDK/
emulator claim, hypothesis and independently verified YESKIA capability. Use the existing
[NPC report format](../../protocol/templates/experiment-report.md), with evidence origin
`synthetic` until a real report is collected. Synchronize clocks or record the uncertainty;
a timestamp correlation alone does not prove a protocol mapping.

`synthetic-cases.json` points to executable fake-stream tests. Reproduce with:

```sh
cargo test -p yeskia-core --test adbian
cargo test -p yeskia-adbian
cargo test -p yeskia-agent-protocol -- --nocapture
```

F01 has host fake evidence for large reads, collision/read-back, uncertain writes and no
replay. S01 has version/request-ID/page bounds and PIM-not-advertised tests. T01 has route
selection tests (`ui/src/deviceRouting.test.ts`) and a plan for independent NPC capture;
there is no real PC Suite comparison yet. Helpers may execute only through explicit Lab
confirmation; no automatically executed experiment batches are shipped.

Only sanitized factual reports should link back to [NPC #6](https://github.com/grayhex/YESKIA/issues/6)
and [serial/protocol #10](https://github.com/grayhex/YESKIA/issues/10). Update STATUS.md and
protocol/COVERAGE.md per actually verified service/model/firmware. Do not claim that an
NPC protocol is implemented from an upstream claim, SDK header or phone-side observation.
