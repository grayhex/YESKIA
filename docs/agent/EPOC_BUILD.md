# EPOC + ADBian build (#30)

Lock: [upstream.lock.json](upstream.lock.json). Audited on 2026-11-11 against real
upstream sources. `epoc` must be checked out as **SDK**, alongside **ADBian**:
ADBian's relative Cargo dependencies refer to `../SDK`. No upstream sources are vendored.

## Build host

Upstream supports Linux **aarch64**, not native Windows. From Windows use a separate
ARM64 Linux machine/VM and copy the resulting packages back. WSL x64 is not a proven
replacement. Windows YESKIA's runtime transport requires neither Python nor WSL.
Build-time Python is required by the upstream SDK tools and this wrapper.

Follow pinned SDK `docs/getting-started.md` for Fedora dependencies:
`gcc gcc-c++ make texinfo bison flex gmp-devel mpfr-devel libmpc-devel openssl-devel
python3-pillow python3-fonttools zlib-devel`. Obtain the official **S60 3rd FP2 SDK**
separately; extract with GnuPoc into `SDK/sdk`. Build GnuPoc host tools into
`SDK/toolchain/host/bin`; apply upstream's `elf2e32-arm-relative.patch` and its documented
modern compiler/OpenSSL fixes. Run `SDK/toolchain/build-cross-gcc.sh` for binutils
2.45 / GCC 15.2. This takes substantial disk space and time.

```sh
python3 scripts/agent/build.py prepare /path/to/agent-build
rustup toolchain install nightly-2026-11-10 --profile minimal --component rust-src
python3 scripts/agent/build.py check /path/to/agent-build
python3 scripts/agent/build.py build /path/to/agent-build
```

The wrapper refuses wrong revisions, tracked modifications, missing Nokia headers,
missing host tools and an unconfirmed host architecture. Device Cargo lockfiles are kept and builds use `--locked`. It selects the locked nightly
in an untracked build launcher, leaving upstream tracked files unchanged, cleans each
app and expects `rshelld.sisx`, `rshell.sis`, `rfprobe.sis`. `yeskia-build.json` records
source revisions, rustc, package sizes and SHA-256, **not installation success**.
Keep a local record of Nokia SDK checksum, host OS, tool binary hashes, GnuPoc revision,
patches and signing certificate fingerprint with the manifest; never upload private keys.
Different signing keys/timestamps may produce different SIS hashes.

## Verification and limitations

This session is Linux x86_64 with no Nokia SDK, cross compiler, signing material or
physical E72. Source checkouts were fetched and inspected. The pinned nightly was installed on the x86_64 host and accepts the target spec (`rustc --print cfg`); this is not a cross build. Rust checks of rshelld, rshell and rfprobe also pass against this ARM target (upstream GUI macros emit an existing cfg warning). The complete C++/link/SIS pipeline is **not yet build-verified**. A passing `prepare` is only a
source-pin check. `check` reports concrete prerequisites; resolve those on ARM64 before
claiming reproducibility. No SIS artifacts or hardware success are fabricated.

An inconsistency exists in upstream comments: `rshelld/app.conf` says not to run GUI
and daemon together, while the current `rshell` GUI implementation/README describe a
P&S-only panel. The pinned GUI device entry constructs `Viewer`, which reads P&S and has no listener;
the old manifest comment is stale. Test coexistence on E72 separately, and do not infer
that an older GUI SIS is also a panel. `rfprobe` opens a separate listener and should be run separately.

## Licensing and updates

EPOC and ADBian are MIT, Copyright (c) 2026 Pizzaria Foundation / Lab2021; preserve their
LICENSE files when distributing derived sources/binaries. Nokia headers/libraries/SDK
are separately licensed and not redistributed. GnuPoc, GCC, binutils, OpenSSL and Rust
have separate licenses; retain notices for distributed tools. A self-signed SIS does
not grant AllFiles or other restricted capabilities on stock firmware.

Update pins in a focused PR after auditing protocol/manifest/tool changes, then rebuild,
record new hashes, rerun host fixtures and E72 bootstrap. Do not follow upstream HEAD
silently or redistribute SDK/ROMs, signing keys or private captures.
