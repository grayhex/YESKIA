# Acceptance ledger for issues #30–36

Host code is checked independently of physical S60 behavior. This ledger deliberately
keeps unfinished device/PIM acceptance visible; source code and CI are not installation
or hardware evidence. The owner has Windows 11 with a laboratory E72; this cloud executor
is Linux x86_64 and does not have remote access to that machine or the Nokia ARM64 SDK.

| Issue | Delivered in the combined foundation | Remaining acceptance |
| --- | --- | --- |
| #30 | Audited pins/licenses, prepare/check/build wrapper, SIS hashes manifest, bootstrap/recovery/compatibility docs | ARM64 toolchain and Nokia SDK setup; validate complete ARM64 build pipeline; build three SIS packages; exact E72 install/capability/launch/reboot results |
| #31 | Rust framing/session/typed file results, Windows paired discovery/SPP SDP/RFCOMM, limits, cancellation, reconnect without replay, synthetic tests | Windows/E72 discovery, live ping/ls/stat/cat, real SDP/provider/timeouts and EOF behavior |
| #32 | FileProvider in Device Core, existing two-pane manager/packages/backups, per-operation routing, collision/read-back, real root probes, Z: read-only, sideload status | Real C:/E:/Z:/ access, permission/root-stat behavior and Bluetooth+USB coexistence. Resume is unavailable; no atomic remote rename is claimed |
| #33 | Developer-only armed Lab, real upstream verbs/raw input, binary replies, explicit mutation confirmation, shared queue/cancellation/privacy | Real read/system/exec smoke; explicit manual test-file destructive cases and optional hard-reset case |
| #34 | Separate versioned one-shot helper, ARM-target Rust check, AppArc bridge source, bounded fake-phone protocol tests/host benchmark, PIM API audit | Helper SIS compile/loader/memory/E72 results. Contacts, native calendar/tasks and SMS agent exports still require separate implementation and read tests |
| #35 | Phone Agent wizard, hash-checked update staging, distinct self-reported install step, manual independent stop/restart/recovery and snapshot/log instructions | Physical first/repeated launch, daemon/panel coexistence, update failure recovery, reboot/autostart/version evidence |
| #36 | Three reproducible case plans, synthetic fixtures/references, evidence/privacy rules linked to NPC lab | Three actual anonymized hardware reports; source PC Suite capture comparison if available; factual #6/#10 links and model-specific matrix evidence |

No issue is automatically closed by this PR. Start the Windows checks in WINDOWS_E72_CHECK.md
using the installer built by CI, retain private output locally, then append only sanitized
facts to COMPATIBILITY.md / research. Do not promote E52/N95/stock devices from E72 results.
