# Native Windows ADBian transport (#31–33)

`yeskia-adbian` is an independent Device Core adapter. It uses Windows Classic Bluetooth,
paired-device enumeration, WinSock SPP SDP service name **rshell** and RFCOMM; no Python,
WSL, Linux L2CAP sockets, COM-port emulation, HTTP endpoint or LAN server. Pairing remains
an explicit Windows step. A paired candidate is not an identified Nokia or a working agent.

The private routing address is kept inside the adapter. UI device IDs are SHA-256-derived
with the installation salt; they do not contain a MAC. Names do not merge USB/Bluetooth
identities. File routes prefer a connected agent, PIM routes prefer native USB/COM; the
file manager can select a specific connected route. Existing USB devices remain usable.

## Discovery and connection

Windows decodes SDP through WSALookupService; returned service names, address families,
addresses and channels are checked. Only channels 1–30 qualify. Try named SPP candidates,
then a channel cached in memory, then a bounded sequential scan. Every successful channel
must give exactly `+rshell ready`; no command is sent to a mismatched service. Reconnect
performs discovery/greeting again, clears Lab arming and never replays the old command.

SDP runs off the UI thread; lookup cancellation uses WSALookupServiceEnd. Nonblocking
RFCOMM select/read/write use 100 ms checkpoints, 2 s per channel, 90 s overall. The agent
page exposes connection cancellation. Operational timeout/cancellation uses the existing
Engine context and queue, shared with file transfers. `ping` is read-only; a heartbeat
proves neither a model nor PIM/file access. The desktop queues read-only heartbeats every 15 seconds for already connected agent
routes, with at most one heartbeat in flight per route. An idle failure is diagnosed on
the next heartbeat/operation; paired-device presence is not radio health.

## Wire format and results

`[u32 big-endian length][payload]`, DATA `.`, terminal OK `+` / ERROR `-`. Partial headers,
partial payloads, EOF, zero/oversized frames, unknown tags and malformed UTF-8 terminal
messages fail the session. Inbound frame limit 512 KiB, accumulated reply 8 MiB, at most
4096 frames. A timed-out/cancelled transaction closes its socket to discard stale replies.
Host errors and Debug hide remote payloads/paths; raw replies are only explicit console
results. A remote ERROR does not promote a capability to available.

`stat` parses size/directory/revision. Only upstream `NotFound` / `PathNotFound` mean
absence; permission/protocol errors must not authorize an overwrite. `ps` is P&S integer
read, **not process enumeration**. `cenrep` and `hal` use the real upstream grammar.
`quit` stops the daemon itself (not just the link) and requires a separate confirmed
Lab action. Unknown `kill`, `rename`, `chmod`, `flash` and raw `put` are rejected. `put` has a dedicated
binary-phase API and GUI action, rather than a Windows command interpreter.

## Files and limits

- The existing two-pane manager, packages and file backups use this FileProvider.
- `/` lists only C:/, E:/, Z:/ whose directory `stat` actually succeeds. Empty upstream
  `ls` can hide NotFound, so it is insufficient evidence of an available disk. A firmware
  which cannot stat roots needs a separately validated drive probe; roots are not invented.
- Reads use 256 KiB byte ranges, verify exact range lengths and recheck size/mtime/attributes.
  Older upstream equal-offset/length parsing is avoided. Files over 1 MiB stream to the
  existing temporary download and SHA-256 verification path. Memory reads are bounded.
- `put` is at most **1 MiB total**, sent as 128 KiB frames, with readiness and final reply.
  Empty files have only one terminal reply. Bigger writes fail before transfer; no fake
  append/chunk-upload workaround bypasses upstream's per-file bound.
- Standard writes are limited to user C:/Data and E:/ paths; ROM Z: is always read-only.
  System writes go through a separately confirmed Lab put, never a background backup.
  Standard Engine writes check collisions and SHA-256 read-back. No atomic remote rename
  is promised. Resume across reconnect is deliberately unavailable; restart an explicitly
  chosen read after revalidating its metadata. Do not append an unverified stale partial.
- `ls` has upstream's fixed UTF-16 listing buffer; very large directories can be incomplete
  at the phone boundary. File metadata is fetched with stat; listing can be slow and is
  cancellable. No model/firmware grants protected directories by assumption.
- On failure after a write starts, Engine returns `uncertain` and asks for content
  verification. No `put`, `mkdir`, `rm`, `exec` or `reboot` is automatically retried.

Sideload sends a checked SIS/SISX/JAR/JAD to `C:/Data/_app_install/` and says **ready to
install on phone**. It never sets `packages.install` or claims installer success.

## Lab profile

Developer mode exposes Device Lab inside Phone Agent. The owner explicitly acknowledges
that the selected phone is their experimental E72 and accepts data/system failure risk.
Arming is scoped to the connection, cleared on disconnect/failure, leaving the page or
switching developer mode off. Backend checks arming again when a queued Lab command
starts. Each mutation shows the exact command, device and risk before explicit execution.
Binary results are shown separately; session history stays in memory, bounded to 30
entries, with explicit private export. A mode switch does not undo an operation already
sent to the phone. No destructive batches run automatically.

Upstream `reboot now` kills the critical file server and forces a reset **without flushing
files**. It is not a graceful restart, factory reset or firmware flashing. Pairing is
link authentication; upstream has no independent application token/per-command auth.
Its DEBUG phone logs can record private arguments even though YESKIA host logs do not.

## Evidence

Golden fixtures: `crates/yeskia-core/tests/fixtures/adbian/wire.json`.
Stream/failure tests: `crates/yeskia-core/tests/adbian.rs`.
Adapter/Engine/fake helper: `crates/yeskia-adbian/tests/adapter.rs`.
UI consent/lifecycle: `ui/src/Agent.test.tsx`. Windows API decoder tests run on Windows CI.
These are synthetic streams and host builds, **not physical E72 evidence**.
