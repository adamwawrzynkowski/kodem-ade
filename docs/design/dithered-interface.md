# Dithered interface

Decision recorded on 8 October 2026. The user asked for a complete visual redesign that removes the generic AI and default GPUI appearance. The direction is a modern macOS workspace that borrows the shading, motion and type of animated dithering and retro software, without becoming a pastiche.

## References and conclusions

- Ordered (Bayer) dithering is a recognizable shading technique that fakes tone with a fixed threshold matrix. Contemporary web uses keep it as a subtle, restrained background treatment. ([Codrops](https://tympanus.net/codrops/2025/07/30/interactive-webgl-backgrounds-a-quick-guide-to-bayer-dithering/), [Paper Shaders](https://shaders.paper.design/dithering))
- Animated dithers flicker when the pattern moves. Keep the pattern fixed to the screen and animate only what is lit. Bayer patterns read as precise; noise reads as organic. ([Playdate developer forum](https://devforum.play.date/t/an-exploration-of-dithering-and-flashing/10272), [Return of the Obra Dinn notes](https://blog.playstation.com/archive/2019/10/17/lucas-pope-on-return-of-the-obra-dinns-art-style))
- Teenage Engineering interfaces use monospace uppercase labels, tabular figures, one signal color and motion that reports state. Nothing OS uses its dot-matrix type for character, not body text, and its accent as an interrupt. ([Teenage Engineering analysis](https://blakecrosley.com/guides/design/teenage-engineering), [Nothing OS](https://nothing.community/d/61774-nothing-os-evolution))
- Critiques of AI-generated interfaces name uniform rounded cards, decorative gradients, soft shadows at one opacity, glass, and status decoration that carries no information. ([Superdesign](https://www.superdesign.dev/blog/why-ai-design-looks-generic), [Noqta](https://noqta.tn/en/blog/ai-design-slop-overused-ui-patterns-fix-2026))

## Rules

| Area | Rule |
| --- | --- |
| Shading | Dither replaces gradients, glow, glass and blur. One-point cells are texture: contact shadows, the dialog scrim, fades where content passes under chrome, and selection grain. |
| Motion | Two- and three-point cells are signal and may animate only while something is working: the Bayer pulse, the composer scanner band, running task rows and the wordmark scan. The new-task field is the one ambient animation, shown only on that screen. Reduced motion renders a static frame. |
| Entrances | Menus and dialogs unroll behind a dithered scan line. Arriving text and context fade in four discrete steps with a short afterglow, without blur. |
| Depth | Surfaces use hairline borders and a one-point top highlight. Floating menus, dialogs, tooltips and the composer add a dithered contact shadow and a tight ambient shadow. Rows and inline content stay flat. |
| Color | Warm graphite (dark) and paper (light) neutrals, one accent for signal and focus, and semantic green and red for changes. The caret and selection use the accent. |
| Type | IBM Plex Sans for prose and controls. JetBrains Mono for code, paths, branches, figures and uppercase tracked labels. The wordmark is a 5×7 dot-matrix mark. |
| Geometry | Radii are 3 (chips), 5 (controls and rows), 7 (code, cards) and 9 (menus and dialogs). Task navigation uses flat two-line rows, not stacked cards. |

## Implementation

`crates/kodem-desktop/src/theme.rs` holds tokens. `crates/kodem-desktop/src/ui/dither.rs` holds the engine: a window-anchored 8×8 Bayer threshold, run-merged quads for small or animated fields, and cached straight-alpha texture tiles for large uniform areas and vertical ramps. Shared controls and the `caps`, `figure` and `line_totals` helpers live in `ui/mod.rs`.

Appearance, motion feel and performance on real displays still need user validation; this change was verified by compilation, formatting and lint checks only, under the current no-visual-testing agreement.
