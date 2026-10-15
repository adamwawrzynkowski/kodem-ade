# Security policy

Kodem runs coding agents on your computer, updates itself from this repository's GitHub releases, and keeps provider sign-ins in the providers' own CLIs. Please report weaknesses in any of these privately.

## Report a vulnerability

Use **Report a vulnerability** on the [Security tab](https://github.com/adamwawrzynkowski/kodem-ade/security/advisories/new) of this repository. Do not open a public issue or pull request for a vulnerability.

Include the affected version (shown in the title bar), the platform, the steps that reproduce the problem, and what an attacker gains. You will get an answer in the advisory. Fixes ship in a new Nightly and Production release.

## Supported versions

Only the newest Production and Nightly releases receive fixes.

## Release integrity

Releases are published only by the repository owner's accounts: a ruleset reserves the release tags for them. Each archive's SHA-256 digest is published with it, and the updater installs an archive only when its size and digest match GitHub's record. Windows executables carry an Authenticode signature; an app signed by a certificate accepts updates only from the same certificate.
