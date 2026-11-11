# Agent compatibility and evidence

| Model | Firmware / signing | Agent | Transport | Build / install / ping / reboot |
| --- | --- | --- | --- | --- |
| E72 (owner laboratory) | Not supplied; record locally | pinned ADBian 0.2.0 source | Windows RFCOMM | not tested / not tested / not tested / not tested |
| E72 stock | Unknown restricted capabilities | same | same | not tested; stock compatibility not claimed |
| E52 | Not tested | none verified | none verified | not tested |
| N95 | Not tested | none verified | none verified | not tested |

Upstream E72 claims are **upstream evidence**, not YESKIA hardware verification.
Each new record needs model, firmware, signing/capability state, source commit, SIS hash,
host OS/build, operation, result/error, time, anonymized report reference. Keep identifiers
and real PIM/file contents local. A new firmware/model requires a new check; never promote
an entire model from one successful `ping` or a synthetic stream.

See EPOC_BUILD.md and E72_BOOTSTRAP.md. Issues #30–36 remain subject to their physical
acceptance criteria even when host code/fixtures pass.
