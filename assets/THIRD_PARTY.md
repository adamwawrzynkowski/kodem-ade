# Third-party notices

Kodem's original code is proprietary; see the root `LICENSE`. Third-party material retains its upstream licenses. The app bundles this document and `LICENSE` in `Contents/Resources/`, with the notices below in `Contents/Resources/licenses/`.

The locked macOS Rust dependency notices are in [Rust-THIRD-PARTY.txt](licenses/Rust-THIRD-PARTY.txt). They include selected license texts, upstream author attribution and original license/notice files. Rust standard-library notices are in [Rust-standard-library.html](licenses/Rust-standard-library.html). Packaging rejects stale dependency notices.

The complete unmodified source of `option-ext` 0.2.0 is included in [option-ext-0.2.0-source.tar.gz](licenses/option-ext-0.2.0-source.tar.gz) under MPL-2.0. Kodem's proprietary terms do not restrict recipients' rights to this source. Build-only `cbindgen` is excluded from distributed dependency notices.

Codex and its packaged helper tools are a separate prerequisite downloaded from OpenAI, not part of the Kodem app bundle. Their licenses and service terms remain separate. The OpenAI logo notice below covers copyright, not trademark permissions.

| Material | Source and adaptation | License |
| --- | --- | --- |
| Embedded syntax definitions and color themes | syntect 5.3.0 default assets; pinned upstream revisions and notices | [Syntax and theme notices](licenses/Syntect-assets.txt) |
| Button, InputBase and supporting state traits | GPUI Kit `gpui-base` 0.6.0. Copied controls use GPUI-CE imports, a reduced state/role adapter and Kodem styling. Unused upstream builder methods remain for the port's component contract. | [Upstream notice](licenses/GPUI-Kit-APACHE.txt), Apache-2.0 |
| Vector controls | Lucide SVGs from GPUI Kit assets 0.6.0 and GPUI Component assets, plus Kodem-authored matching vectors and the Kodem mark. | [Lucide ISC and Feather MIT notices](licenses/Lucide-ISC.txt), [GPUI Kit notice](licenses/GPUI-Kit-APACHE.txt) |
| OpenAI provider logo | Simple Icons 15.0.0; see [source record](licenses/openai-icon-source.txt). | [CC0](licenses/simple-icons.txt) |
| Native text input | `gpui-pre` 0.3.4's GPUI input example, adapted to GPUI-CE 0.2.2. Kodem adds multiline layout, wrapping, read-only selection, syntax colors and task draft integration. | [GPUI license](licenses/GPUI-APACHE.txt), Apache-2.0 |
| Geist Mono Regular, Medium, SemiBold and Bold | [Geist upstream](https://github.com/vercel/geist-font); static font files, unchanged | [SIL Open Font License](licenses/geistmono-OFL.txt) |

Interface text uses the macOS system font, which is not bundled. The original complete GPUI Kit library is not linked: its GPUI-pre type graph is incompatible with this application's GPUI-CE renderer.
