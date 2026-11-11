# Agent lifecycle (#35)

Phone Agent is part of the existing Fluent desktop workspace. Its steps separate initial
transfer, installation on the phone, pairing, an actual agent response and update staging.
There is no update-on-connect and the agent package is unrelated to the Windows installer.

1. Before experimenting, use the existing File Backups service for accessible user files
   and the agent configuration directory. A file snapshot is not a full system/ROM backup.
   Verify it locally and retain the previous SIS with SHA-256 and signing/build metadata.
2. Bootstrap without an agent: use available WPD/Mass Storage/OPP or card/local download,
   then finish installation on the phone. If the route is unavailable, follow manual
   E72_BOOTSTRAP.md steps. Never claim seamless stock-firmware installation.
3. Pair in Windows, connect (SDP + greeting), check response. `pong` proves a live agent,
   not its version. Base ADBian has no protocol version/capabilities verb; the optional
   YESKIA helper provides its **own** version only, not rshelld's version.
4. For update, choose a local SIS/SISX, verify the displayed source hash and target. A
   confirmed transfer creates `_app_install`, sends once and read-back-verifies the file.
   The GUI says ready to install. Collision/error/Bluetooth loss means inspect the file;
   it does not claim that a new version was installed.
5. Explicitly stop/restart the daemon **on the phone** using an independent control/install
   channel. A running daemon can prevent replacing its own executable. Do not self-update
   through the process providing the only management connection. Install/reboot/reconnect
   and compare independently available build evidence; base ping cannot verify an update.
6. The GUI's “completed installation on phone” checkbox is **self-reported**, separate from
   transport verification. Reboot/autostart, optional GUI coexistence and actual firmware
   capability grants need a local hardware report each; no checkbox promotes the matrix.
7. Export session/logs only by explicit choice. Retrieve `C:/Data/_logs` through the shared
   FileProvider; these logs may contain private paths/arguments. Keep crash evidence local,
   strip personal identifiers before publishing a technical report.

Failure/recovery instructions and compatibility table are in E72_BOOTSTRAP.md and
COMPATIBILITY.md. E52/N95/stock firmware have no inherited E72 verification. Do not retry
failed installs indefinitely; record error code, agent/firmware/signing state and the
independent install channel. Owner service recovery stays outside YESKIA; no automatic
flashing is included. The physical first/repeated launch, reboot and autostart acceptance
checks remain pending on the owner's Windows/E72 setup.
