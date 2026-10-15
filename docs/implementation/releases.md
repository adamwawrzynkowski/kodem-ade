# GitHub releases and updates

The three build commands embed a channel and version, package both executables, and publish complete GitHub releases. The desktop checks its installed channel, downloads on request, verifies the bundle, and installs after saving state and stopping the idle host.

The release commands require an authenticated GitHub CLI account with write access to `adamwawrzynkowski/kodem-ade`. The updater can read public releases without authentication. For this private repository, users need GitHub CLI installed and `gh auth login` completed with repository read access. Tokens are read at request time and never embedded in a build.

Debug builds do not check for updates. Nightly checks only prereleases tagged `nightly/vX.X.X`; Production checks stable releases tagged `vX.X.X`. Versions compare numerically. Release commands select the workspace version or increment the latest channel's patch version, whichever is newer. `KODEM_VERSION=X.X.X` selects an explicit newer version. Releases contain `Kodem-macos-aarch64.zip`; GitHub's asset digest is required before an update is offered.

Downloaded bundles must match the channel, version, bundle identifier and repository. macOS code signatures are verified. The current build uses ad hoc signing, as before; Apple Developer ID signing and notarization are separate distribution requirements. SHA-256 authenticates the downloaded content against metadata retrieved from GitHub over HTTPS; an ad hoc signature does not establish publisher identity.

The app must be installed in a writable directory to replace itself. A download is staged on the same filesystem as the installed app. Finish active tasks and checks before applying the update; the host rejects shutdown while either is active. Applying an update closes host terminals. Drafts and settings are flushed before shutdown. The helper waits for the desktop to exit, replaces the app with rollback protection, and asks macOS to reopen it. Installation errors are retained in the staging directory's `install.log`.

## Verification on 10 September 2026

- Workspace compilation and Clippy with warnings denied passed. The pinned `block` dependency emits Cargo's existing future-compatibility notice.
- The actual Debug command built and signed `target/debug/Kodem Debug.app` and verified both Apple Silicon executables. The sandbox prevented `iconutil` from producing the icon; the same command passed outside the sandbox without code changes for that step. Neither executable was launched.
- The updater's authenticated HTTPS transport read the private GitHub release list. No releases existed and none were created during verification.
- Temporary headless checks passed for numeric version ordering, channel isolation, embedded channel/version labels, invalid release metadata, version allocation and rejection, publication digest checks, save acknowledgement ordering, failed saves, busy-host shutdown refusal, installer success, signature failure, reopen failure, and waiting for desktop exit. Installation fixtures substituted only signing and opening commands; they did not operate on an installed app.
- Shell syntax and final diff checks passed. Temporary tests and fixtures were removed.

An actual release upload/download/restart cycle and visual interaction remain untested. Optimized release packaging was not executed because publication requires a clean committed checkout already on GitHub. No commit or push was made. App control, launch and screenshots were excluded by the working agreement.

The release integration follows [GitHub release metadata](https://docs.github.com/en/rest/releases/releases), [asset digests and downloads](https://docs.github.com/en/rest/releases/assets), and [GitHub CLI release creation](https://cli.github.com/manual/gh_release_create).

## Draft publication lookup repair

The first Nightly build uploaded its archive but failed with HTTP 404 when querying the unpublished release through the REST tag endpoint. Publication now resolves the draft with `gh release view --json apiUrl` and reads its asset digest from that release URL. A read-only probe of the actual publication function reproduced the old failure and passed with the corrected lookup; creation and publication were intercepted during that probe.

The existing `nightly/v0.1.0` draft was then published after its source commit and archive checksum were verified. Its downloaded asset matched the local archive byte-for-byte. Workspace compilation, shell syntax and diff checks passed. Temporary probes and downloads were removed. This verifies release publication and download; app installation, reopening and visual interaction remain untested.

## Windows and Linux packages

Recorded on 15 October 2026. Each platform builds natively: `scripts/build-macos.sh` on macOS, `scripts/build-linux.sh` on Linux, and `scripts/build-windows.sh` from Git Bash on Windows (`Build *.cmd` at the repository root). GPUI compiles its Direct3D shaders with the Windows SDK's `fxc.exe` in release builds, so Windows releases cannot be cross-compiled from macOS.

| Platform | Release asset | Package folder | Identity and metadata |
| --- | --- | --- | --- |
| macOS | `Kodem-macos-aarch64.zip` | `Kodem.app`, `Kodem Nightly.app` | `Info.plist`, ad hoc code signature |
| Windows | `Kodem-windows-x86_64.zip` and `.sha256` | `Kodem`, `Kodem Nightly`, `Kodem Debug` | `release.json`, Authenticode signature, embedded icon and version resource |
| Linux | `Kodem-linux-x86_64.tar.gz`, `.sha256` and optional `.asc` | `kodem`, `kodem-nightly`, `kodem-debug` | `release.json`, optional detached OpenPGP signature |

`release.json` carries `name`, `identifier`, `version`, `channel`, `repository` and `build`. The Windows and Linux updaters require the installed folder's `release.json` to name the running channel, accept only archives whose entries are regular files and folders under the channel's package folder, then compare identifier, version, channel and repository. On Windows, when the running `kodem.exe` has a valid Authenticode signature, both new executables must be validly signed by the same certificate thumbprint. The detached installer is `scripts/install-update-windows.ps1` or `scripts/install-update-linux.sh`. It waits for the desktop to exit, retries the folder rename while the stopped host releases its executable on Windows, swaps folders with rollback and starts the new app. Windows can leave the stage folder holding only `install.log`, which the installer process keeps open.

A release build of a commit that already has a published release in its channel adds its archive to that release instead of allocating a new version. A draft release for the commit stops the build. An upload whose GitHub digest differs from the local archive is deleted from an existing release, or leaves a new release as a draft.

Release builds on Windows refuse unsigned executables unless `KODEM_ALLOW_UNSIGNED=1`. Linux signing is optional unless `KODEM_REQUIRE_SIGNING=1`. License notices for Windows and Linux are generated at build time under `target/licensing/<target>/` with the macOS policy; the Linux graph needed reviewed clarifications for `harfrust`, `wayland-protocols-plasma`, `xim-ctext` and `xim-parser`. The Windows graph has no MPL dependency, so its package carries no `option-ext` source archive.
