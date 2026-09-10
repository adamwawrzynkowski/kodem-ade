# Licensing maintenance

Kodem's original code is proprietary under the root LICENSE. Workspace crates
are marked `publish = false`. Third-party code, copied controls, fonts, icons,
and syntax assets retain their upstream licenses.

Normal builds run `python3 scripts/licenses.py prepare` before compiling. This
checks the notice snapshot against Cargo.lock, all workspace manifests, the
target policy, the notice generator, and the asset notices. It also checks the
generated notices and MPL source archive. Current tracked notices are reused.
If stale or missing, notices are regenerated under `target/licensing/` with the
existing license policy and reused on subsequent builds until inputs change.
The generated build notices replace their tracked counterparts in the app bundle.
Tracked files stay unchanged, preserving release clean-checkout checks.

Generation installs cargo-about 0.9.2 under `target/license-tools/` if that
version is unavailable, and installs the active toolchain's rust-docs component
if its standard-library notice is missing. These setup steps may need network
access; the license scan uses the locked, locally fetched dependency graph.
Unknown licenses, missing clarifications and unsupported MPL source requirements
still stop the build. Address those review failures before retrying.

`python3 scripts/licenses.py check` remains a strict offline check of the
tracked snapshot. Use the maintenance workflow below to update tracked notices;
ordinary builds do not require a separate refresh or generated-file commit.

After a dependency or licensing change:

1. Run `cargo fetch --locked` to populate the local package cache.
2. Install the pinned scanner with `cargo install cargo-about --version 0.9.2 --locked --features cli`
   if it was not already installed by a build. The generator reuses the build-local copy.
3. Review the licenses of added or updated dependencies. `about.toml` accepts
   permissive licenses and permits MPL-2.0 only for `option-ext`.
4. Run `python3 scripts/licenses.py refresh`. The scan itself is offline and
   uses the exact locked graph for Apple Silicon macOS, excluding build-only
   and development dependencies. Missing copyright notices require a
   version-specific entry in `clarifications.json`; do not invent copyright
   holders or approve an unknown license to silence a failure.
5. Review the generated notices, source archive and snapshot, then run the
   debug build to verify packaging. Keep these files with the dependency change.

The generator preserves Cargo authors, selected license texts, and original
root LICENSE, COPYRIGHT, NOTICE and AUTHORS files. Version-specific
clarifications retain retrieved upstream text or a recorded MIT declaration
when upstream supplies no copyright statement. Generated notices contain no
local filesystem paths. The complete unmodified `option-ext` source is shipped
alongside its MPL notice, so recipients do not depend on an external source URL.
New MPL components require explicit source-packaging work.

`Syntect-assets.txt` records the upstream revisions used by syntect 5.3.0's
embedded syntax definitions and themes. Review it when upgrading syntect.
Other copied or embedded assets still require their own provenance review;
Cargo metadata is not a complete asset-license audit.

The current bundle contains Kodem and kodem-host. Codex and its helper,
ripgrep and shell are downloaded separately from OpenAI by the build setup.
They are not covered by the Rust dependency notice snapshot or relicensed by
Kodem's LICENSE. Before adding those binaries to an installer or app bundle,
review the exact runtime package's redistribution terms and third-party notices.
Provider service terms and logo trademark permissions are separate from
source-code licensing.

Changing Kodem's current notice does not revoke license grants accompanying
previously distributed copies. New proprietary terms do not override upstream
licenses or third-party source rights.
