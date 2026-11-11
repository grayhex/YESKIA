# E72 bootstrap and recovery

Status: **no physical phone connected to this build environment**. This is a procedure,
not evidence of success. Use the owner's experimental E72; acceptance of laboratory
risk does not turn an untested stock phone into a compatible one.

1. Record local model, RM, firmware, signing state, capability policy and agent build
   manifest. Use an alias in shareable reports; keep addresses, identifiers and logs local.
2. Save accessible user files and agent configuration. Prepare an independent install
   channel (card/USB Mass Storage/WPD transfer or manually served local download).
   Do not replace USB drivers to obtain Bluetooth access. On stock firmware, installation
   may fail because `LocalServices AllFiles ReadDeviceData WriteDeviceData PowerMgmt`
   cannot be granted. Record the exact installer/loader error; owner-prepared modified
   firmware/installserver is a separate manual procedure, never an automatic YESKIA step.
3. Transfer `rshelld.sisx`, optional current `rshell.sis`, and separate `rfprobe.sis`.
   A transfer only means **ready to install on phone**. Compare SHA-256 locally.
4. Install using the phone screen. Pair in Windows Bluetooth settings (Classic Bluetooth).
   Reboot and check daemon auto-start independently of first launch. Launch optional panel;
   check P&S status/logs and verify it does not compete for the listener.
5. Connect YESKIA: paired device → SPP SDP name `rshell` → RFCOMM greeting
   `+rshell ready` → `ping` gives `pong`. Only then test `ls C:\Data`, `stat` and a
   synthetic file read. A correct greeting is not PIM/version/capability negotiation.
6. Record three separate outcomes: installed, running, responds over transport. Record
   reboot/auto-start and reconnect independently. Do not publish personal phone output.

## Failure path

- No package: resolve build prerequisites in EPOC_BUILD.md; a host test is not a SIS build.
- Install/security error: retain exact local code, check firmware/signing/capability grants.
  Do not repeatedly retry or claim self-signing unlocks restricted capabilities.
- No SDP: check daemon, pairing/radio and panel/logs. Run rfprobe separately. If an old
  GUI owns the listener, stop it on phone before launching daemon. Cache may be stale.
- Greeting mismatch: do not send commands to that service; rediscover SDP.
- Broken auto-start: manually launch/repair with the independent install channel. Do not
  claim panel availability proves boot startup.
- Failed update: keep the previous SIS/hash and user snapshot. Stage the new SIS, stop
  daemon and install/restart **on phone**, reconnect and verify version with an extended
  agent (upstream ADBian has no version command). Reaching `pong` alone cannot prove update.
- Nonbooting experimental phone: use the owner's documented service/recovery procedure
  outside YESKIA. No automatic firmware flashing, guessed firmware or ROM redistribution.

Raw upstream debug logs may include command paths/arguments. Treat them as private even
when host logs are redacted. Pairing provides link-level access, not per-command consent
or an application authentication token. Avoid leaving the privileged daemon exposed.
