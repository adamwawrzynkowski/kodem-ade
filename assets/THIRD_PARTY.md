# Third-party notices

Kodem's original code is proprietary; see the root `LICENSE`. Third-party material retains its upstream licenses. The app bundles this document and `LICENSE` in `Contents/Resources/`, with the notices below in `Contents/Resources/licenses/`.

The locked macOS Rust dependency notices are in [Rust-THIRD-PARTY.txt](licenses/Rust-THIRD-PARTY.txt). They include selected license texts, upstream author attribution and original license/notice files. Rust standard-library notices are in [Rust-standard-library.html](licenses/Rust-standard-library.html). Packaging rejects stale dependency notices.

The complete unmodified source of `option-ext` 0.2.0 is included in [option-ext-0.2.0-source.tar.gz](licenses/option-ext-0.2.0-source.tar.gz) under MPL-2.0. Kodem's proprietary terms do not restrict recipients' rights to this source. Build-only `cbindgen` is excluded from distributed dependency notices.

Codex and its packaged helper tools are a separate prerequisite downloaded from OpenAI, not part of the Kodem app bundle. Their licenses and service terms remain separate. The OpenAI logo notice below covers copyright, not trademark permissions.

| Material | Source and adaptation | License |
| --- | --- | --- |
| Embedded syntax definitions and color themes | syntect 5.3.0 default assets; pinned upstream revisions and notices | [Syntax and theme notices](licenses/Syntect-assets.txt) |
| Button, InputBase and supporting state traits | GPUI Kit `gpui-base` 0.6.0. Copied controls use GPUI-CE imports, a reduced state/role adapter and Kodem styling. Unused upstream builder methods remain for the port's component contract. | [Upstream notice](licenses/GPUI-Kit-APACHE.txt), Apache-2.0 |
| Vector controls | Lucide SVGs from GPUI Kit assets 0.6.0 and GPUI Component assets (among them the Git workspace's `triangle-alert`, `eye-off` and `chevron-up`), the Lucide drawings that the Liquid Git client bundled for the Git workspace (`git-branch`, `git-commit-horizontal`, `git-merge`, `tag`, `clock`, `refresh-cw`, `arrow-down-to-line`, `arrow-up-from-line`, `key-round`, `circle-dot`, `pilcrow`, `list-ordered`), and `list-end`, `ship-wheel` and the Review workspace's `scan-eye` from Lucide 1.55.0 (`lucide-static`), with the geometry unchanged, plus Kodem-authored matching vectors and the Kodem mark. | [Lucide ISC and Feather MIT notices](licenses/Lucide-ISC.txt), [GPUI Kit notice](licenses/GPUI-Kit-APACHE.txt) |
| GitHub mark in the Git workspace | The GitHub drawing of GPUI Component's assets (`github.svg`), which follows the Simple Icons GitHub icon. The notice covers copyright, not trademark permission. | [CC0](licenses/simple-icons.txt) |
| Project glyphs | Lucide 1.55.0 (`lucide-static`): 225 icons in `assets/icons/projects/`. Each file is the upstream drawing on one line with its shape attributes only; the geometry is unchanged. | [Lucide ISC and Feather MIT notices](licenses/Lucide-ISC.txt) |
| OpenAI provider logo | Simple Icons 15.0.0; see [source record](licenses/openai-icon-source.txt). | [CC0](licenses/simple-icons.txt) |
| Anthropic logo for the Claude Code provider | Simple Icons 16.35.0; see [source record](licenses/anthropic-icon-source.txt). The notice covers copyright, not trademark permission. | [CC0](licenses/simple-icons.txt) |
| Google logo for the Antigravity provider (`antigravity.svg`) | The Simple Icons Google drawing, unchanged. The notice covers copyright, not trademark permission. | [CC0](licenses/simple-icons.txt) |
| Native text input | `gpui-pre` 0.3.4's GPUI input example, adapted to GPUI-CE 0.2.2. Kodem adds multiline layout, wrapping, read-only selection, syntax colors and task draft integration. | [GPUI license](licenses/GPUI-APACHE.txt), Apache-2.0 |
| Fira Sans Regular, Medium, SemiBold, Bold, Italic and Bold Italic | [Google Fonts repository](https://github.com/google/fonts/tree/e345593da2a4d596212542edbd28f2ed08fe6cbe/ofl/firasans), version 4.203; static font files, unchanged | [SIL Open Font License](licenses/firasans-OFL.txt) |
| IBM Plex Mono Regular, Medium, SemiBold and Bold | [Google Fonts repository](https://github.com/google/fonts/tree/01d5d9f05246fb7090a8403b81257535cb1a088b/ofl/ibmplexmono), version 2.3; static font files, unchanged | [SIL Open Font License](licenses/ibmplexmono-OFL.txt) |
| Inter Regular, Medium, SemiBold, Bold, Italic and Bold Italic | [Inter 4.1 release archive](https://github.com/rsms/inter/releases/download/v4.1/Inter-4.1.zip), `extras/ttf`, font version 4.001; static font files, unchanged | [SIL Open Font License](licenses/inter-OFL.txt) |
| JetBrains Mono Regular, Medium, SemiBold and Bold | [JetBrains Mono 2.304 release archive](https://github.com/JetBrains/JetBrainsMono/releases/download/v2.304/JetBrainsMono-2.304.zip), `fonts/ttf`; static font files, unchanged | [SIL Open Font License](licenses/jetbrainsmono-OFL.txt) |
| Fira Code Regular, Medium, SemiBold and Bold | [Fira Code 6.2 release archive](https://github.com/tonsky/FiraCode/releases/download/6.2/Fira_Code_v6.2.zip), `ttf`, font version 6.002; static font files, unchanged. The licence text is the `LICENSE` file of the [6.2 tag](https://github.com/tonsky/FiraCode/blob/6.2/LICENSE) | [SIL Open Font License](licenses/firacode-OFL.txt) |

Characters that the bundled fonts lack fall back to macOS system fonts, which are not bundled. The original complete GPUI Kit library is not linked: its GPUI-pre type graph is incompatible with this application's GPUI-CE renderer.
