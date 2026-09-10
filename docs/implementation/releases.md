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
