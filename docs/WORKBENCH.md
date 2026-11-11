# E72 preparation, TLS and local contacts (#41–43)

The Fluent UI and local Rust/Tauri architecture are retained. No service, account,
Python runtime or HTTP client is added. The new tools are reached from the Home
page (Prepare E72), Certificates (TLS / HTTPS Revival), and Contacts.

## TLS / HTTPS Revival

Porting reference: `grayhex/s60modern` commit
`efaf53d1390e6d6d380f52427cae1ede91de7212`,
`symport/revive/tls.py` and its manifest. Its MIT notice is retained in
`docs/licenses/s60modern.txt`. The manifest is reviewed data, not an updater.

The built-in pins are MBedTLS 1.4.4 (926,820 bytes,
`d20d6d41c796d53fe1c0df413248c14a91e8c5b018ad8c71194e66bf60937a27`)
and ssl.zip v21 (9,428 bytes,
`62365db422253e87b0328058e160ab03c3a3501e7eeae064596727307be9b416`).
They were downloaded from the exact HTTPS author URLs on 2026-11-11 and matched
both pins. SIS UID/version/targets/header checksum/controller and data CRC were
checked. ZIP contains only `sys/bin/ssl.dll`. This is host validation, **not E72
installation evidence**. Downloaded binaries are neither committed nor bundled.
Source licensing and binary distribution rights are shown separately from the
manifest; no signature is implied for the unsigned MBedTLS package.

Open author download uses the system browser with a fixed manifest URL. Import
validates size/hash first, then bounded SIS/ZIP structure. A changed upstream
binary fails closed pending an explicit reviewed manifest update. Prepared files
are content-addressed under the app's private data directory; every reuse
revalidates them. No URL supplied by an import is fetched or executed.

Compatibility is platform/documentation evidence for E52/E72/N95/N95 8GB/N97 mini,
with exact firmware still unverified. N8/E7 remain explicit manual author-support
cases. Qt is excluded; EKA1/7610 needs its separate BearSSL work. Protocol model
identity is distinct from a user-entered bench profile or Bluetooth display name.

SIS staging uses the existing device queue/FileProvider, creates
`C:/Data/MBedTLS.sis` without overwrite and requires read-back verification. It is
not installation. Installation is confirmed on the phone and recorded only as an
owner report. DLL extraction is local only. Automatic privileged DLL replacement
and automatic rollback are unsupported until rights, replacement and recovery
are verified on the actual firmware. The manual guide requires preservation of
the original C: DLL (or recording its absence and preserving Z: separately),
InstallServer/Open4All, manual application/reboot and a phone HTTPS test. Z: is
never a write target. ZIP staging cannot produce an installed/verified/rolled-back
state. Snapshot/application/test/manual-restoration reports are explicitly manual.
The phone clock comparison is also manual; host HTTPS does not test phone TLS.

## Local address book

The local book is independent from the phone snapshot. Rust handles bounded VCF
2.1/3.0/4.0 imports (folding, quoted-printable, charset, multivalue fields and raw
unknown properties), CSV quoted cells/newlines and UTF-8/UTF-16/Windows-1251.
CSV mapping is previewed before import; every source column is retained, including
overlapping manual mappings. Google-style labels and common Outlook phone labels
are preserved. Photos/unknown properties survive export but are never fetched or
rendered as remote content. Partial cards are retained with warnings; malformed
inputs do not replace the current book.

JSON (`yeskia.contacts/1`) and YESKIA CSV with its structured sidecar are lossless
local backups. VCF3 export preserves known fields and unknown properties; local
provenance/status/IDs are not portable vCard metadata. CSV display cells beginning
with spreadsheet formula prefixes are escaped; the structured sidecar retains
the original value. Exports require a user-selected **new** filename.

Create/edit/duplicate/delete, multivalue fields, search/sort, multiselection,
group assignment and undo operate locally. Duplicate suggestions never mutate
data. Merge preview lets the selected open contact be primary; scalar conflicts
remain in `X-YESKIA-MERGED-*` fields and all multivalues are retained. Contacts
never enter diagnostics via Debug/errors. The local SQLite document uses
optimistic revisions, transactions and a bounded 20-edit history, surviving restart.
The database is local plaintext, with the same host access boundary as app data.

Phone reads reuse the real native PCCS reader. Before/after connection context
must match. Snapshots retain native IDs, source fingerprint, raw returned fields,
read time and content hash. Stable local snapshot IDs permit a field diff against
the previous read. Copying selected snapshot records creates independent local
records; it never writes to the phone. Real Create/Update/Delete or group writes
remain **unsupported**: no unverified PCCS opcodes were invented. There is no
synchronization or success claim for phone writes. Existing SMS/calendar readers
remain separate.

## E72-1 first-run bench

The initial owner profile is E72 / RM-530 / 054.005 / Symbian 9.3 (S60 3rd FP2),
not a detected assertion. The user verifies and edits it on the phone. Progress is
scoped by salted device fingerprint, firmware, RM, OS, model, transport,
conservative driver-set hash, YESKIA mode and TLS manifest hash. A changed context
loads separate progress; record requests carry an expected key to reject stale
updates. Offline reports cannot become device evidence.

Typed steps cover battery/SIM inspection, identity, connection/Doctor, accessible
file backup, separate PIM contacts snapshot, 2.4 GHz WPA2, owner-entered operator
APN on the phone, 2G/3G without LTE, clock, certificates, manual rights, TLS and
local contacts. Every related-page link reaches a working existing tool. No APN,
passwords, SIM secrets, purchase flow or app catalog is added to this wizard.

Only native operations can produce `verified`: an actual protocol probe, a
complete verified FileProvider backup captured for the active context, or a
native PCCS contacts snapshot captured for that context. A contacts snapshot is
not a full SMS/calendar/system backup. Manual/blocked/not-supported/not-started
choices cannot submit `verified`. Readiness exports include safe platform/status
and evidence-source data, without identifiers/fingerprints, paths, contacts,
APN or passwords. `ready` stays false while any required step lacks actual proof;
a complete set of manual checkmarks is not device acceptance.

## Validation and remaining hardware work

Rust tests exercise import/export/unknown fields/encoding/formula safety,
optimistic edits/first-save undo/restart, manual merges, snapshot identity,
malicious ZIP and changed pins, SIS identity/version/CRC, platform exclusions,
manual proof rejection and context invalidation. UI tests cover explicit local
save/undo, export path selection, manual confirmation, report privacy and TLS
deep links/blocked automatic apply. Windows release CI remains required.

Owner has Windows 11 and a lab E72; this cloud environment is Linux x86-64 and
has no remote connection to that phone. Actual TLS install/HTTPS/recovery,
PCCS contact snapshot on 054.005, rights and backup/reconnect acceptance remain
not tested. Firmware 091.004 requires new evidence. There is no automatic release,
deployment, phone mutation or hardware pass inferred from CI.
