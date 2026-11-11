# Optional upstream packages for an existing laboratory E72

If there is no ARM64/Nokia SDK build machine yet, upstream publishes
[ADBian v0.2.0](https://github.com/pizzaria-foundation/adbian/releases/tag/v0.2.0).
This is an upstream distribution, not a YESKIA-built or hardware-verified package.
The annotated tag resolves to commit `c017ce9a7f0cdf8e0743e301470cdd9675935cee`;
YESKIA's audited source pin is a separate later commit in upstream.lock.json.
Do not label these binaries as output of our pinned build wrapper.

Downloaded locally on 2026-11-11 and SHA-256 matched GitHub's asset digest:

| Package | Bytes | SHA-256 |
| --- | ---: | --- |
| [rshelld.sisx](https://github.com/pizzaria-foundation/adbian/releases/download/v0.2.0/rshelld.sisx) | 38488 | `c8c7fce95d4cccc6a24fe1b5692c00d19fdbde01b95c1473a428044071340cab` |
| [rshell.sis](https://github.com/pizzaria-foundation/adbian/releases/download/v0.2.0/rshell.sis) | 73724 | `58f745ed94609f1fe48db9c71f3deabd15fd3493281ea8896a5221218ccfcbac` |

Verify the hash after download, preserve the previous package, then follow
E72_BOOTSTRAP.md. A matching hash is integrity evidence only. Signing/capability policy
may still prevent installing or starting on stock firmware. No package has been installed
on the owner's E72 by this cloud session. The optional YESKIA helper is separate and has
no prebuilt SIS in this repository; its C++/SIS build still requires the Nokia SDK.
