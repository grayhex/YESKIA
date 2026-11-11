# Windows 11 + laboratory E72: first hardware check

Use the exact Windows installer artifact from the combined PR's successful CI run. Keep
its commit/hash. These checks do not build/install a phone SIS; an existing rshelld must
already be installed/running. If not, follow EPOC_BUILD.md / E72_BOOTSTRAP.md first. PREBUILT.md lists independently hash-checked upstream release packages when no build host is available.

1. Leave the current USB installation/drivers unchanged. Pair the E72 in Windows Bluetooth
   settings, keep it nearby with Bluetooth on, and open YESKIA → Phone Agent.
2. Select the paired candidate and connect. Record separately: candidate found, SDP named
   service found or fallback, greeting accepted, ping response. Never publish MAC/IMEI or
   raw captures. If connection fails, retain the exact sanitized error code and whether
   daemon/panel is running. Cancel connection if needed; do not change USB drivers.
3. Open Files. Select the Bluetooth route if multiple are connected. Record which C:/E:/Z:/
   roots actually appear; an absent root is not assumed accessible. List C:/Data and choose
   a nonpersonal synthetic test file to download. Compare local SHA-256 and exact size.
4. Explicitly confirm a small synthetic upload to C:/Data/YESKIA-Test (create it through Lab
   mkdir if needed), check read-back. A >1 MiB upload must be rejected; a >1 MiB download
   from a prepared E: test file should use ranges. Do not use personal files for the report.
5. Interrupt Bluetooth during a test upload. Expected: failure/uncertain, no automatic replay.
   Inspect the phone before any manual retry; reconnect and verify Lab is disarmed. Check
   the independent USB/PCCS route remains usable if it was working before.
6. For Lab tests enable Developer mode, explicitly acknowledge this experimental E72,
   arm the selected connection and start with pwd, ping, ls, stat, hal and a known P&S/
   CenRep key. `ps` reads a key, not process lists. Find/grep on a small test directory.
   Confirm exec only for a known owner-prepared test executable. rm only explicit test
   files. **Do not run reboot now as part of this first check**; it needs a separate owner
   decision because it forcibly resets through the critical file server.
7. For Sideload choose a known SIS <=1 MiB, inspect its SHA-256 and target. Expect “ready to
   install”, then finish on the phone separately. Running rshelld must be stopped/restarted
   through an independent phone channel before replacement. A base pong is not version proof.
8. Test phone reboot/daemon auto-start and optional panel coexistence separately after
   saving local data. Base ADBian has no version query. Optional YESKIA helper version applies
   to the helper only; do not treat it as rshelld's installed version.

Return a **sanitized** result per numbered step: YESKIA commit, Windows build, E72 firmware/
signing state, agent SIS/source version, operation, success/error code and concise technical
observation. Redact identifiers, file names/content and app/PIM data. Keep raw logs/captures
local. No previous test in this repository constitutes these hardware results.
