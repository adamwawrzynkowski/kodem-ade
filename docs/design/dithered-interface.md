# Dithered interface

Decision recorded on 8 October 2026. The user asked for a complete visual redesign that removes the generic AI and default GPUI appearance. The direction is a modern macOS workspace that borrows the shading, motion and type of animated dithering and retro software, without becoming a pastiche.

Revision recorded on 12 October 2026. The user asked for Kodem to use the same dithering, aesthetics and animations as Liquid, their Git client. Kodem now uses Liquid's visual language: its palette, its dot-matrix dither fields, its mono labels, its button surfaces, and its effect layer. The first version of this document described fine one-point grain as the main texture. That grain remains only for shadows, scrims and fades.

Second revision recorded on 12 October 2026. The user reported that performance was very poor and that many elements did not follow the new language: blocks, borders, foldable sections, diffs, popups, windows and badges. This revision adds the rules for those elements, the switch and slider controls, and the rendering rules in "Rendering cost".

Third revision recorded on 12 October 2026. The user asked for a small bar of icons on the left side that swaps the window between workspaces: Agentic Coding, Chat and Review. This revision adds the rules for that rail and for the change of workspace.

Fourth revision recorded on 12 October 2026. The user asked for the email addresses of signed-in accounts to be covered by dither by default and shown on a click, then for the cover to hide only a part of the address, because a full cover did not look like something to click. This revision adds the rule for private text.

Fifth revision recorded on 12 October 2026. The user reported that the icons of projects did not fit: a flat tinted square with a thin outline glyph stood next to tiles that show an application icon. This revision adds the rule for project tiles.

Sixth revision recorded on 13 October 2026. The user reported that the conversation looked plain and messy and asked for a redesign that takes ideas from other agentic coding tools. This revision adds the rules for reading text and for the actions of a message, and it changes the rule for inline code.

## References and conclusions

- Ordered (Bayer) dithering is a recognizable shading technique that fakes tone with a fixed threshold matrix. Contemporary web uses keep it as a subtle, restrained background treatment. ([Codrops](https://tympanus.net/codrops/2025/07/30/interactive-webgl-backgrounds-a-quick-guide-to-bayer-dithering/), [Paper Shaders](https://shaders.paper.design/dithering))
- Animated dithers flicker when the pattern moves. Keep the pattern fixed to the screen and animate only what is lit. Bayer patterns read as precise; noise reads as organic. ([Playdate developer forum](https://devforum.play.date/t/an-exploration-of-dithering-and-flashing/10272), [Return of the Obra Dinn notes](https://blog.playstation.com/archive/2019/10/17/lucas-pope-on-return-of-the-obra-dinns-art-style))
- Teenage Engineering interfaces use monospace uppercase labels, tabular figures, one signal color and motion that reports state. Nothing OS uses its dot-matrix type for character, not body text, and its accent as an interrupt. ([Teenage Engineering analysis](https://blakecrosley.com/guides/design/teenage-engineering), [Nothing OS](https://nothing.community/d/61774-nothing-os-evolution))
- Critiques of AI-generated interfaces name uniform rounded cards, decorative gradients, soft shadows at one opacity, glass, and status decoration that carries no information. ([Superdesign](https://www.superdesign.dev/blog/why-ai-design-looks-generic), [Noqta](https://noqta.tn/en/blog/ai-design-slop-overused-ui-patterns-fix-2026))
- Liquid (`/Users/adamwaw/Projects/Personal/liquid`, `src/theme.rs` and `src/ui/{dither,fx,motion,liquid_button,components}.rs`) is the source of the current language.

## Rules

| Area | Rule |
| --- | --- |
| Color | Five surface steps: backdrop, chrome, panel, header and elevated. Graphite and Paper are Kodem's own dark and light themes; twelve classic editor themes supply the same steps from their own palettes. One accent that stays vivid at low dot coverage; each theme has its own, and the accent setting replaces it. Green, red, amber and blue carry change, failure, attention and information, in the theme's own hues. |
| Type | Fira Sans for prose and titles. IBM Plex Mono, a face of a different design, for buttons, labels, figures, paths, branches and code. Section titles are small uppercase mono labels followed by a dotted rule. |
| Dot fields | Shading is drawn as separate square dots on a two- or three-point pitch from one 8×8 Bayer threshold. A field gives the density at each dot: Fade, Glow, Flow, Wave, Sweep, Pointer, Dissolve, and products of them. Dots may take three alpha steps. |
| Controls | A button is a mono label on a surface with static dots that shade one edge, more dots that light up under the pointer, and a dithered ring that spreads from the press point. Neutral, ghost, filled and tinted buttons differ in fill and tone, not in shape. Only keyboard focus draws an accent edge; a click does not leave a control outlined. |
| Switches and sliders | An on/off setting is a switch with a dot-matrix track and a square thumb. A number in a range is a slider with a dot-matrix track, tick marks and a mono readout. A choice between a few named options is one segmented group. Settings do not use step buttons. |
| Selection | A selected row carries an accent tint, an accent edge, and dots that fade from its leading edge over a fixed width of 64 points, so that they thin out before the row's text. It has no outline. A selected tab has a two-point accent line with a dithered glow above it. |
| Blocks | Code, tool output, an inline diff, a table and a callout share one block: an inset surface with a hairline edge, and a header with a dithered marker, a mono label, a dotted rule that fills the width, then counts and actions. |
| Rules and edges | Inside a surface, separators are dotted rules. A solid hairline marks only the outer edge of a surface, the bottom of a block header and the split between panes. |
| Folds | The header of a foldable section reads as a section header: a chevron, a mono label, its totals, a dotted rule and a duration or count. |
| Readouts | A key and its value are joined by a dotted leader. A measured share is a mono label with its figure, a dot-matrix meter and a caption line. Popovers have no bold sans-serif headings. |
| Badges | States, modes and identities are tinted mono chips. Inline code is mono text in the reading color on a plate of fine dither with square corners. Only a link or a file reference takes the accent. Only a file reference sits on a dotted baseline, in the accent, which says that a click opens it. |
| Reading text | All prose of a message starts on one leading edge; a plate that continues on the next line hangs three points into the margin and does not move its paragraph. Space shows what belongs together: 6 points between a title or an introducing sentence and what follows it, 10 between paragraphs and lists, 12 around code, tables and quotations, and 18 or 24 above a title. Titles are semibold in three sizes with a line height of 1.3; the two largest stand on a dotted rule. A short paragraph of bold text that a block follows is a title of the third size. The items of a list with several paragraphs stand further apart. A list inside a list has open square bullets. |
| Message actions | A message of the user starts a turn and has 18 points above it. The reply that ends a turn keeps its copy action in a row below it. A reply that work follows is a step of that work: its copy action appears under the pointer in the padding beside the reply and takes no room. |
| Private text | In the email address of an account, the name before the `@` is covered by a still field of dense dots as wide as the name; the domain stays readable. The cover takes the accent and the domain brightens under the pointer. A click on the address shows the name and the next click covers it again. Every address is covered again when a popup, a dialog or a settings section opens, and when the app starts. A covered name is not drawn below its cover. |
| Diffs | A changed line has a tinted background and a solid two-point edge in its color. File headers show the path and a micro diffstat. |
| Motion | Everything that repeats advances on one stepped clock of about twelve frames per second: dot fields, the working pulse, the text gleam and the blink of a waiting glyph. The clock also runs while the window is unfocused and rests only while the window is hidden. A row moves while its task works. Menus and dialogs slide a few points while a Bayer dissolve in their own color clears from the top. Short confirmations spring in and settle. |
| Chat background | The chat lies on a field of four wide, soft lights in two colors of the theme: one at each side, a wide glow below the composer, and a faint one that wanders along the bottom. Each drifts slowly around its place and swells a little; the speeds share no period, so the field does not repeat. Readability comes first: a light has no edge and no detail, its tint at the center is about a tenth of full alpha on dark surfaces and less on light ones, and the lights rest outside the centered reading column. A veil of the surface color with soft edges lies over the column from the top of the chat to the composer and takes three quarters of any light behind it. Behind messages the tint stays below 6% on dark themes and 4% on light ones in every state, which costs text at most 15% of its contrast at the brightest point of a working task. The field follows the open task: quiet and slow while idle; brighter, in the accent and livelier while the agent works; amber and breathing while the task waits for the user; calm green after a finished turn; red after a failure. A change of state blends colors and movement within about a second. This is the one place that uses blur instead of dots, because dots behind text cost legibility. There is no hero field on the new-task screen. |
| Panels | The sidebar, the changes panel and the terminal slide open and closed in about a fifth of a second. A panel is clipped to its current size while it moves. |
| Workspace rail | A column of icon tiles on the window's leading edge, on the sidebar's surface and separated from it by a dotted rule. The tile of the open workspace carries the accent tint, an accent edge, static dots along its bottom, and a two-point accent bar on the rail's leading side. The other tiles are bare icons that light up under the pointer. A small square in amber, the accent or green on a tile says that a thread of that workspace waits, works or has an unread result. The rail has no labels; a tooltip names the workspace and its keys. |
| Workspace change | The tile of the workspace that opens forms out of a Bayer dissolve. The thread feed and the main pane clear out of a dissolve in their own surface color, in about a quarter of a second. Nothing slides, because the panes keep their place. |
| Project tile | A tile without an image is a dark plate with the corners of an application icon, in the light appearance too, so that it looks like the image tiles in the same list. The tint of the project colors the plate, its edge and a still glow of one-point dots in the leading top corner. The glyph is paper white, the color of the mark in Kodem's icon. Review and chat tiles use the same plate. |
| Arriving text | A new letter forms out of dither. Its pixels appear in Bayer order on a grid fixed to the window, a few loose dots flicker in its cell while it forms, and it starts a little brighter than its final color. A typed letter shows a third of itself at once and is complete in about a tenth of a second, on a one-point grid. A streamed letter starts from almost nothing and takes about a quarter of a second, on a grid of about one and a half points. Every text of an agent arrives this way: replies, thoughts, notices and tool output. A text that comes whole forms from its start to its end in a sweep, at most 96 letters at a time and 576 letters in about one and a half seconds; the text before those letters is there at once. |
| Effects | One layer above the window draws short effects: a ripple on press, an impact (flash, expanding frame, dithered shockwave and sparks) where something important lands, a chip that flies or glides between two parts of the UI, and a crumble where something is discarded. |
| Texture behind text | Dots do not lie behind text. A card or a message bubble carries its dots in a band of 16 points along its bottom edge, inside its padding, and not over a share of its height. |
| Depth | Surfaces use hairline borders. Floating menus, dialogs, tooltips and the composer add a fine dithered contact shadow. There are no blurred glows, glass or gradients on surfaces; the chat background is the exception. |
| Geometry | Radii are 4 (chips), 6 (controls and rows), 8 (cards, code and the composer) and 10 (menus and dialogs). |
| Reduced motion | With the macOS Reduce Motion setting, dot fields render one frame, entrances jump to their final state, and no effect is drawn. |

## Where effects fire

| Event | Effect |
| --- | --- |
| A press on any button | Ripple in the button's tone |
| A new task is created | A chip with the first prompt line flies from the composer to the thread feed and lands with an impact |
| A message is queued during a turn | A chip glides from the composer into the queue tray |
| A turn completes, fails, or starts waiting for input | Impact on that task's thread row, in green, red or amber |
| A task is deleted | Its thread row crumbles |

A flying chip must stay in view. Its arc rises unless that would carry it into the title bar or past the top of the window; then it dips below its ends instead.

An effect lands only on a target that was painted in the current or the previous frame. A row that scrolled out of view gets no effect at its old position.

## Rendering cost

The first Liquid version painted one quad for each dot on each frame and redrew the window on every display frame while anything moved. In a debug build that made scrolling, typing and hovering slow. These rules keep a frame cheap:

| Rule | Reason |
| --- | --- |
| A static dot field is baked once into a sprite and cached. | A cached field costs one draw call. A fade along one axis uses a strip that serves every size along the other axis, so all buttons of one height share one sprite. |
| A frame bakes at most 2 MB of new sprites. The cache holds 48 MB and drops the least recently used sprites. | A field that misses the budget is painted as quads for that frame. A window resize cannot fill memory. |
| Fields that move, follow the pointer or dissolve are painted as quads in one layer, on small areas only. | A layer gives all its dots one draw order. Without it each dot is sorted against the whole scene. |
| No element repeats an animation on every display frame. Repeating decoration reads the stepped clock. | Every redraw renders the whole workspace, so a spinner at 120 frames per second costs 120 workspace renders. |
| The effect layer and the chat background are siblings of the workspace, and the workspace is a cached view. | A frame that only moves an effect or the background reuses the workspace's last paint. The background draws each light as one blurred shape on the GPU, so a frame costs five shapes with the veil and no per-pixel work on the CPU. It moves sixty times a second while the agent works or a state blends in, and thirty times a second otherwise, so a long task does not redraw the window at the full rate of a fast display. |
| A panel's slide re-renders the workspace for its fifth of a second only. | The slide runs on a short timer that stops when every panel has reached its place. |
| An empty state's emblem is still. | A moving emblem would render the workspace on every step while it is on screen. |
| Streamed text redraws about thirty-five times per second while its letters form. | That gives a letter about nine steps of dither. More redraws would render the conversation more often for little gain. |
| The letter reveal has no custom shader. A forming glyph is drawn through clip rectangles, one per run of lit cells in a row, in one layer. | GPUI exposes no shader hook. Runs keep a streamed reply near two thousand small draws per frame at most, and a settled letter is ordinary text again. |
| Parsed Markdown and tool-call views are cached until their message changes. | A conversation renders its visible messages again on every frame. |
| Dependencies are optimized in the debug profile. | GPUI's layout, text and scene code are most of a frame. |

## Implementation

`crates/kodem-desktop/src/theme.rs` holds tokens. `crates/kodem-desktop/src/ui/dither.rs` holds the engine: the `Dither` element and its fields, the dotted rule, the dot-matrix ratio bar, and the fine window-anchored grain for shadows, scrims and fades. `ui/fx.rs` holds the effect layer and its anchors. `ui/backdrop.rs` holds the chat background. `ui/history.rs` holds the rail of sent messages and its preview. `ui/motion.rs` holds entrances. `ui/workspaces.rs` holds the workspace rail and the change of workspace. `ui/code_review.rs` holds the Review start screen and the hand-over of findings. `ui/kit.rs` holds the shared pieces that screens compose: control textures, section headers, chips, keycaps, the micro diffstat, meters, icon tiles, glow headers, blocks and their headers, dotted leaders, the selection edge and empty states. `ui/controls.rs` holds the switch and the slider. `ui/censor.rs` holds the cover of account email addresses. The button builders are in `ui/mod.rs`.

The engine and the effect layer are ports of Liquid's modules to GPUI-CE 0.2.2 and to Kodem's own `Theme`. They do not use GPUI Component.

Appearance, motion feel and performance on real displays still need user validation. These changes were verified by compilation, formatting and lint checks only, under the current no-visual-testing agreement. The rendering rules follow from reading the code paths of a frame; no frame time was measured.
