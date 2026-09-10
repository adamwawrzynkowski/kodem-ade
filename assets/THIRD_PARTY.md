# Third-party notices

Kodem's original code is proprietary; see the root `LICENSE`. Third-party material retains its upstream licenses. The app bundles this document and `LICENSE` in `Contents/Resources/`, with the notices below in `Contents/Resources/licenses/`.

The locked macOS Rust dependency notices are in [Rust-THIRD-PARTY.txt](licenses/Rust-THIRD-PARTY.txt). They include selected license texts, upstream author attribution and original license/notice files. Rust standard-library notices are in [Rust-standard-library.html](licenses/Rust-standard-library.html). Packaging rejects stale dependency notices.

The complete unmodified source of `option-ext` 0.2.0 is included in [option-ext-0.2.0-source.tar.gz](licenses/option-ext-0.2.0-source.tar.gz) under MPL-2.0. Kodem's proprietary terms do not restrict recipients' rights to this source. Build-only `cbindgen` is excluded from distributed dependency notices.

Codex and its packaged helper tools are a separate prerequisite downloaded from OpenAI, not part of the Kodem app bundle. Their licenses and service terms remain separate. The OpenAI logo notice below covers copyright, not trademark permissions.

| Material | Source and adaptation | License |
| --- | --- | --- |
| Embedded syntax definitions and color themes | syntect 5.3.0 default assets; pinned upstream revisions and notices | [Syntax and theme notices](licenses/Syntect-assets.txt) |
| Button, InputBase and supporting state traits | GPUI Kit `gpui-base` 0.6.0. Copied controls use GPUI-CE imports, a reduced state/role adapter and Kodem styling. Unused upstream builder methods remain for the port's component contract. | [Upstream notice](licenses/GPUI-Kit-APACHE.txt), Apache-2.0 |
| Vector controls | Lucide SVGs from GPUI Kit assets 0.6.0, plus Kodem-authored matching vectors and the Kodem mark. | [Lucide ISC and Feather MIT notices](licenses/Lucide-ISC.txt), [GPUI Kit notice](licenses/GPUI-Kit-APACHE.txt) |
| OpenAI provider logo | Simple Icons 15.0.0; see [source record](licenses/openai-icon-source.txt). | [CC0](licenses/simple-icons.txt) |
| Native text input | `gpui-pre` 0.3.4's GPUI input example, adapted to GPUI-CE 0.2.2. Kodem adds multiline layout, wrapping, read-only selection, syntax colors and task draft integration. | [GPUI license](licenses/GPUI-APACHE.txt), Apache-2.0 |
| IBM Plex Sans Regular, Medium and SemiBold | [IBM Plex upstream](https://github.com/IBM/plex/tree/master/packages/plex-sans/fonts/complete/ttf); static font files | [SIL Open Font License](licenses/ibmplexsans-OFL.txt) |
| JetBrains Mono variable font | Google Fonts, `ofl/jetbrainsmono/JetBrainsMono[wght].ttf` | [SIL Open Font License](licenses/jetbrainsmono-OFL.txt) |
| Manrope ExtraBold | Static weight-800 instance of Google Fonts `ofl/manrope/Manrope[wght].ttf`, generated with fonttools 4.59.2 and named Manrope ExtraBold | [SIL Open Font License](licenses/manrope-OFL.txt) |

The fonts are renamed locally for stable include paths. IBM Plex Sans and JetBrains Mono font data is unchanged. Manrope is instantiated as a static ExtraBold font so the native renderer uses its intended weight. The original complete GPUI Kit library is not linked: its GPUI-pre type graph is incompatible with this application's GPUI-CE renderer.
