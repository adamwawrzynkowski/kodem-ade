# Third-party notices

The native build resolves its dependency versions through `Cargo.lock`. Cargo packages retain their upstream licenses in the package cache. The app bundle includes the license files in `Contents/Resources/licenses/`.

| Material | Source and adaptation | License |
| --- | --- | --- |
| Button, InputBase and supporting state traits | GPUI Kit `gpui-base` 0.6.0. Copied controls use GPUI-CE imports, a reduced state/role adapter and Kodem styling. Unused upstream builder methods remain for the port's component contract. | [Upstream notice](licenses/GPUI-Kit-APACHE.txt), Apache-2.0 |
| Vector controls | Lucide SVGs from GPUI Kit assets 0.6.0, plus Kodem-authored matching vectors and the Kodem mark. | [Lucide ISC and Feather MIT notices](licenses/Lucide-ISC.txt), [GPUI Kit notice](licenses/GPUI-Kit-APACHE.txt) |
| OpenAI provider logo | Simple Icons 15.0.0; see [source record](licenses/openai-icon-source.txt). | [CC0](licenses/simple-icons.txt) |
| Native text input | `gpui-pre` 0.3.4's GPUI input example, adapted to GPUI-CE 0.2.2. Kodem adds multiline layout, wrapping, read-only selection, syntax colors and task draft integration. | [GPUI license](licenses/GPUI-APACHE.txt), Apache-2.0 |
| IBM Plex Sans Regular, Medium and SemiBold | [IBM Plex upstream](https://github.com/IBM/plex/tree/master/packages/plex-sans/fonts/complete/ttf); static font files | [SIL Open Font License](licenses/ibmplexsans-OFL.txt) |
| JetBrains Mono variable font | Google Fonts, `ofl/jetbrainsmono/JetBrainsMono[wght].ttf` | [SIL Open Font License](licenses/jetbrainsmono-OFL.txt) |
| Manrope variable font | Google Fonts, `ofl/manrope/Manrope[wght].ttf` | [SIL Open Font License](licenses/manrope-OFL.txt) |

The fonts are renamed locally for stable include paths. Font data is unchanged. The original complete GPUI Kit library is not linked: its GPUI-pre type graph is incompatible with this application's GPUI-CE renderer.
