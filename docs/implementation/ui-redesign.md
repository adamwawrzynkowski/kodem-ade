# Whole UI redesign

The user rejected the initial UI on 9 September 2026 and explicitly authorized app visual testing for this session. This replaces the prior visual-testing restriction for this work. The original supplied Zuse, T3 Code and Synara screenshots guide organization. Their recorded attachment paths are in the originating thread; the rejected Kodem screenshot is the baseline, not the target.

## Work sequence

1. Ground the design in the original screenshots and inspect native rendering. Complete.
2. Replace the visual system and whole workspace shell, then navigation and task creation. Complete.
3. Replace conversation/composer, review presentation, secondary panels and terminal chrome. Complete.
4. Compile, inspect actual GPUI Metal-rendered windows, exercise interaction paths and correct defects. Complete.
5. Remove temporary visual fixtures and probes, rebuild both bundles, install the release and record verified limits. Complete.

## Design contract

The workspace uses an integrated 38 px title/toolbar, a 260 px project sidebar, a readable conversation column with a docked composer, and a contextual review pane. Empty workspaces open into task creation with a repository picker and composer. No always-empty review column, promotional headline or duplicate global status toolbar.

Navigation uses icon/label rows, search revealed by its icon or ⌘F, project headers and indented task rows. Tasks use one compact line with a branch icon, title and age, including when selected. Empty projects do not reserve a placeholder row. Window controls, branding and panel actions share the same aligned chrome.

Conversation uses a user message bubble, plain assistant blocks, compact tool disclosures and low-emphasis message actions. The composer has a shallow raised edge, a 58 px minimum text area, a compact context row and a clear send button. Model and effort choices belong next to the input.

Review opens on a full-height directory-aware file list with status badges and change totals. Choosing a file opens its diff; a back button returns to the list. Comparison tabs, staging and tinted code rows remain available. Refresh preserves the selected file by path. The feedback gutter remains independent of text selection. Inline feedback entry retains surrounding code. Metadata and whole-file staging continue to use real snapshot identity and stale-content checks.

All panels use one icon family, native system text and bundled monospace code, shared spacing, subdued borders, clear focus/hover/pressed states and the same neutral dark/light palettes. Management panels organize related settings in labeled sections with aligned controls. File preview and terminal output get the available width.

Backend ownership, provider delivery and Git contracts remain intact. The redesign changes UI composition and interactions; new display state stays in Shell or existing preferences. No speculative integration statuses or sample conversations appear in normal app data.

## Visual verification

macOS currently denies the shell Accessibility and Screen Recording access. GPUI-CE provides a native Metal `render_to_image` test API for the application's own rendered scene. Temporary visual-check instrumentation launched Kodem in an isolated data directory, exported actual rendered windows and dispatched app-local events. It did not inspect another application's pixels or input. Fixtures are explicitly labeled and kept out of user data.

Final release installation must preserve active user work. The old installed window can close only after its task/draft state is safe; the installer retains its bundle backup. Temporary test data and instrumentation are removed before the final normal builds.

## Confirmed native checks

Inspected empty and populated workspaces, dark/light preferences, the file browser and highlighted source preview, unified and split diffs, inline feedback, model and skills menus, terminal output, task creation and 950 × 700 layouts. The captures use GPUI's actual Metal renderer; OS-drawn traffic lights and window shadows are outside those app-scene exports.

App-local mouse and keyboard assertions covered file selection, tool disclosure, search typing and clearing, repeated sidebar toggles after search focus, theme switching, Escape dismissal, Changes toggling at narrow widths, task creation, gutter selection of new-side lines 12–13, inline comment typing and successful feedback queuing. The Stage control staged only src/retry.rs in the temporary Git repository. A terminal command printed KODEM_VISUAL_OK. No model turn or external MCP tool was executed.

Visual checks caught missing inherited SVG colors, disappearing keyboard focus after a control left the tree, an off-screen split column and wrapping diff gutters. These were corrected at their owners and inspected again. Terminal input acknowledgements no longer enqueue redundant workspace refreshes.

The real runtime catalog also exposed a macOS socket issue: accepted sockets inherited the nonblocking listener flag, and a large response ended after 8,192 bytes. Host connection workers now explicitly use blocking streams. After restarting the isolated host, the complete 20,890-byte catalog with 53 entries parsed successfully and appeared in the redesigned resource panel.

The broader acceptance limits remain in [progress.md](progress.md). Temporary instrumentation, fixture databases, Git repositories and captures are removed before normal final builds.

Both normal builds and Clippy passed. The release is installed and opened; the saved user task and draft digest matched across the idle-host restart. See the final delivery checks in [progress.md](progress.md).

## Compact reference correction

The subsequent screenshot comparison rejected the first redesign's density. The correction removes repeated assistant identity headings, sizes user bubbles to their content, groups consecutive tool messages into one disclosure, aligns chat and composer to one 940 px column, and uses smaller regular text with a near-black green-neutral palette. Navigation adds explicit New chat and New project rows. Search is available by icon or shortcut and closes with Escape.

Before the user stopped visual testing, a populated native scene was inspected and app-local checks confirmed file-list navigation, split mode, search input and Escape dismissal, sidebar toggles and narrow-width review switching. The user then withdrew permission for further visual testing. All native inspection stopped, and the temporary driver, fixtures and captures were removed. Subsequent changes use source review and compilation only; final visual acceptance remains with the user.

The missing tool runner was caused by copying a standalone Codex binary. Build commands now install and validate the official complete versioned package. An ephemeral authenticated CLI check executed pwd and read a fixture README successfully. This does not establish an end-to-end turn through Kodem's provider UI.

## Live activity and motion

The accepted compact layout is retained. The host now reports transient, typed activity separately from the saved transcript. Item identities track concurrent work and completion; new turns, terminal events and worker exit clear stale activity. Reasoning events provide a status label only. Their content is not copied into activity text. Command labels use the provider's parsed read/search actions instead of guessing from shell text. Older TaskView responses remain readable through a default empty activities field.

A bounded activity strip shows each current operation, a brief command/tool detail and elapsed time. Fifteen activity kinds cover reasoning, response writing, reading/searching files, commands, file edits, web search, planning, tools, context compaction, agent coordination, review, image inspection/generation and timed waits. Starting, general work and requests for input have explicit fallback labels. Active task titles, tool groups and streaming indicators share a moving text highlight; work icons rotate or pulse. Native GPUI text rendering animates the highlight across glyphs without a separate shader dependency.

Dialogs, review panes, sidebar expansion, menus and terminal panels have short entrance animations. Dismissal immediately removes their controls. The macOS Reduce Motion preference is read through NSWorkspace at startup and during workspace refresh. GPUI renders all animation wrappers statically when that preference is enabled. objc2-app-kit 0.3.2 was already in the lockfile and is now a direct macOS dependency with the needed accessibility features.

Text-button builders require an explicit icon. Primary actions, approvals and comparison tabs retain icon-plus-text labels. Changes, diff mode, file staging, file references, skills, feedback removal and size adjustments use icon controls with accessible names and tooltips.

Verification uses compilation and source checks only. Temporary reducer tests passed concurrent-item completion, duplicate-start timestamps, turn cleanup, ignored non-work events and parsed-command classification. Event names and fields were checked against the installed pinned runtime's generated schema. Temporary tests and schemas were removed. No app launch, visual test or authenticated provider turn was performed for this change; animation appearance and timing remain unverified visually.

## Comparison dropdown

The four comparison buttons now live in a single compact dropdown in the review header. Its trigger displays the selected comparison and chevron beside the change count. The old 34 px toolbar row is removed. The dropdown marks the selected option and closes on selection, outside click, Escape, or hiding review with Cmd-D. Selecting another comparison clears the prior file selection and pending snapshot before requesting its diff.

Clippy, debug compilation/packaging and release compilation/packaging passed for the dropdown correction. Release installation was blocked by the running installed app or host; the new bundle is ready in target/release/Kodem.app. No app control or visual testing was performed.

## Liquid redesign, 12 October 2026

The user asked for a strong redesign in the fashion of Liquid, their Git client: its dithering, its aesthetics and its animations. They also asked for better use as an agentic coding tool, Claude Code support, several accounts per provider, a default model and account per project, and nicer output. The visual language is recorded in [dithered interface](../design/dithered-interface.md). The survey behind the interaction changes is in [research](../design/research.md). Provider and account behavior is in [providers](providers.md).

### What changed in the desktop

| Area | Change |
| --- | --- |
| Design system | Liquid's palette, Geist Mono labels, dot-matrix dither fields, button surfaces, dissolve entrances and the effect layer replace the earlier warm palette, IBM Plex Sans, JetBrains Mono and fine grain. Shared pieces live in `ui/kit.rs`. |
| Title bar | Project and branch, a worktree chip, a button that opens the next task that needs the user, a running readout, and a field that opens the command palette. |
| Thread feed | Sections lead with what needs a person: Needs you, Working, Ready for review, then Threads and Settled. A finished result is unread until its task is opened. Rows show the provider, a dot for a non-default account, the branch and a micro diffstat. The footer shows the composer account's limit windows. |
| Command palette | Command-K lists every action, thread and account in one ranked list. |
| Keyboard | Next and previous task, Command-1 to Command-9, the next task that needs the user, the agent menus, Shift-Tab to cycle access, Control-backtick for the terminal, and Command-Return to send the other way during a turn. |
| Composer | One chip for provider, model and account, one for effort and one for access. The send button offers stop on an empty draft during a turn and steer or queue on a draft. The agent's step list is a progress strip. `@` completes files of the task's checkout. |
| Agent menu | Provider rail, the provider's accounts with plan and limit meter, models, and a control that makes the current choice the project's default. |
| Requests | Permission requests, plan reviews and questions render inside the composer. A decline can carry feedback. Questions support several choices. The separate approval dialog is removed. |
| Transcript | A run of tool calls folds into one summary line with counts, totals and a live line for the running call. Failed calls stay visible while the run is folded. Structured calls show a category, a subject, a duration, and an inline diff for edits. Thinking is a foldable row. One-line notices read as dividers. |
| Markdown | Bare web addresses become links. A click opens a link, and a click on a file reference opens it in the file browser. GitHub alerts render as callouts. Code blocks show their line count. |
| Notices | Errors and short confirmations float above the workspace instead of taking a row under the title bar. The task's account shows a notice when it is signed out or at its limit, with the action that fixes it. |
| Settings | An Accounts section with one card per account. Projects show and clear their default agent. The provider section points to accounts for sign-in. |

### Verification

Compilation, formatting and Clippy are the only checks for the desktop. No application launch, screenshot or visual test was performed, under the standing agreement. Appearance, layout at narrow widths, animation timing, effect positions and performance of the dot fields are unverified and remain with the user.

Risks to check first on a real display:

1. Dot fields cost one quad per dot. The hero field on the new-task screen and the active thread rows repaint about twelve times per second.
2. Effect targets come from the last painted frame. An effect whose target is hidden is skipped or lands on a fallback box.
3. Menus opened from the keyboard attach to the composer chip that was painted last, or to a position near the composer when none was.
4. A click on a link opens it only when the pointer did not move between press and release, so that a drag still selects text.

## Follow-up, 12 October 2026

The user tested the Liquid redesign and reported five things: models showed "Default" and "recommended" entries and Devin was still a provider; many elements did not follow the new language; performance was very poor; some settings needed sliders and toggles instead of step buttons; and compaction and automatic compaction were missing.

### Performance

No frame time was measured, because the application may not be launched under the standing agreement. The causes below come from reading what one frame runs. `scripts/build-macos.sh debug` builds the unoptimized profile, so every one of them was several times more expensive there.

| Cause | Change |
| --- | --- |
| Each dot of each dither field was one quad on each frame, and each quad was sorted against the scene. | Static fields are baked into cached sprites (`ui/dither.rs`). Moving fields paint quads in one layer on small areas. |
| The working pulse, the wordmark scan, the text gleam and the blinking glyph requested a redraw on every display frame. Every redraw renders the whole workspace. | They read one stepped clock of about twelve frames per second. |
| A press ripple or a flying chip redrew the workspace on every frame of the effect. | The effect layer is a sibling of the workspace, and the workspace is a cached view (`Stage` in `ui/chrome.rs`). |
| A visible run of tool calls built a view of every call on every frame. That removed escape sequences from each call's whole output and cloned every message of the run. | Tool views are cached until their message changes (`tool_output.rs`). Only the calls on screen are copied. |
| Each visible message was parsed as Markdown on every frame. | Parsed blocks are cached by text (`markdown.rs`). |
| An expanded tool call parsed its output as JSON on every frame. | The view holds the pretty-printed output. |
| Streamed text redrew the conversation on every display frame while letters settled. | It redraws about thirty times per second. |
| The hero field on the new-task screen ran three full-size animated layers. | The noise layer is static and cached. The wave and the pointer glow still move. |
| GPUI and the other dependencies were unoptimized in the debug profile. | `[profile.dev.package."*"]` sets `opt-level = 3`. Kodem's own crates stay unoptimized. The first debug build after this change takes several minutes longer. |

### Design consistency

`ui/kit.rs` gained the block and its header, the dotted leader and the selection edge. `ui/controls.rs` holds the switch and the slider, ported from Liquid. Every screen was moved onto these recipes: the transcript (code, tables, callouts, quotes, inline code, tool runs, inline diffs), the usage panel and its hover cards, the agent menu, menus, the command palette, the completion menu, the review pane and diff view, the changes tree, the file browser, questions and approvals, the plan and activity strips, attachments and queued messages, the agent popup, the image preview, dialogs, and Settings. A control shows its accent edge only for keyboard focus; a click no longer leaves it outlined.

### Settings

On/off settings are switches. Sidebar width and conversation text size are sliders with a readout. Choices between a few options are segmented groups. The event switches under Notifications are disabled while task notifications are off.

### Models

No menu shows a provider default. The composer always resolves a concrete model: the choice of this draft, the project's default, the model this account used last in this project, the model it used last anywhere, then the provider's default model. The host records the last model per account and per account and project (`Settings::model_choices`).

### Compaction

The usage panel has a Context section with the window, the point where automatic compaction starts, and Compact now. The composer's `/compact` command and the command palette run the same request. Settings → Providers holds the rules: one for all providers, an optional rule per provider, and an optional rule per model.

### Verification

Compilation, formatting and Clippy are the only checks. Appearance, layout, motion and the size of the performance gain are unverified.

## Second follow-up, 12 October 2026

The user reported seven things after using the build.

| Report | Cause | Change |
| --- | --- | --- |
| The look of inline code blocks | Every code span was accent text on an accent chip, which is loud in a paragraph with many spans. | Code is text-colored mono on a quiet neutral chip. Links and file references keep the accent. |
| Icons in some buttons are off center | The button builders added the label as a child even when it was empty. The empty child took a gap and pushed the icon aside. | An empty label is not added (`ui/mod.rs`). |
| The animated wave is slow and unwanted | The hero field on the new-task screen. | Removed. A reactive dot field behind the whole conversation replaces it (`ui/backdrop.rs`). It is a cached view beneath the workspace, so its steps do not render the conversation. |
| The KODEM logo | The 5×7 dot-matrix wordmark. | The title bar shows the Kodem mark and the name in Geist Mono. |
| Effects fly outside the window | An arc rose whenever its chip stayed inside the window by half a chip, which let it pass through the title bar. | An arc keeps a margin of the title bar plus one and a half chips from the top and dips instead (`arc_control` in `ui/fx.rs`). |
| Panels are hard to resize | The dividers were one point wide and were also their own hit area. | Each divider has an eleven-point hit area drawn above both panes, with a line that lights up under the pointer. |
| The send button jumps when the composer narrows | All toolbar items were in one wrapping row. | The options wrap in their own group. Stop and send are a fixed group on the trailing side. |

Verified by compilation, formatting and Clippy only. The look of the background, its densities and speeds, and the new arcs are unseen.

## Third follow-up, 12 October 2026

| Report | Change |
| --- | --- |
| The chat background makes text hard to read | The first change removed the field from behind the text. The user wanted it kept there with a look that leaves text readable. The field now covers the whole chat: behind the centered reading column it is one-point specks at 30% of the alpha and 80% of the density, and over 130 points beside the column it grows to two-point dots at full alpha. |
| "K" and "Kodem" doubled in the title bar | The title bar shows the name only. |
| Inline code blocks still do not fit the interface | A span is mono text on a plate of fine dither with square corners and a dotted baseline, both drawn by the shader. The earlier change had only replaced the fill color. |
| The composer toolbar is messy; plan mode sits inside the approval choices | The context and usage meters moved out of the toolbar, first to a row above the draft and then, at the user's request, into their own tab on the composer's top edge: a surface with a slanted leading side that is drawn as a path one point over the composer's border (`paint_meter_tab` in `ui/chat.rs`). The toolbar has three sections closed by dotted rules: attachments and skills; model, account and effort; mode and approvals. A provider with a plan mode has a Mode chip (Build or Plan) and a separate Approvals chip. The provider has one mode at a time, so while Plan is on the approval rule is remembered in the window (`Shell::approval_memory`) and applied when the task returns to Build. Shift-Tab switches Build and Plan. |

The approval rule remembered during plan mode is not saved across a restart; after a restart a task that leaves plan mode returns to the provider's asking mode. Verified by compilation, formatting and Clippy only.

## Fourth follow-up, 12 October 2026

| Report | Change |
| --- | --- |
| The drifting dot background is still disliked; make it tiles that blink slightly with dithering | `ui/backdrop.rs` is a grid of 24-point tiles. Each tile has its own phase and period from a hash of its position. While its cycle is above a threshold it is lit: a wash fades in and dots fill it in Bayer order. The mood sets the color, the share of time a tile is lit, the speed, a phase per row that makes blinks climb, and how much the tiles blink together. The view is still cached beneath the workspace. |
| Panels should animate open and closed | `motion::Reveals` holds how far the sidebar, the changes panel and the terminal are open. `Shell::sync_reveals` runs once per render and moves them toward what the settings ask for on a 16-millisecond timer that stops when they arrive. No toggle site changed. Panels take their state at once until the workspace has loaded, and with reduced motion. The changes panel does not slide in a window at most 1100 points wide, where it replaces the chat. Dialogs and menus already dissolve in; they still close at once. |

Verified by compilation, formatting and Clippy only.

## Fifth follow-up, 12 October 2026

The context and usage meters have their own tab on the composer's top edge (`paint_meter_tab`).

The user asked for a shader effect in which dithering creates the letters, for typing and for streamed replies, smooth, performance first, faster for typing. GPUI-CE 0.2.2 has no hook for custom shaders, so `components/streaming_text.rs` builds the effect from what the renderer has: each forming glyph is painted through clip rectangles that cover the grid cells whose Bayer threshold its progress has passed. Lit cells of a row merge into one rectangle, loose dots are plain quads, and a line's forming letters share one layer. A typed letter uses a one-point grid, starts at 34% and takes 110 ms. A streamed letter uses a grid of about one and a half points, starts at 10% and takes 260 ms; streamed text redraws every 28 ms. The Bayer table moved to `bayer.rs` so that the text components and `ui/dither.rs` share it.

Not measured: the cost of the clip-rectangle draws on a fast stream, which is bounded by the 96 letters that may form at once. Verified by compilation, formatting and Clippy only.

## Sixth follow-up, 12 October 2026

| Report | Change |
| --- | --- |
| The tiled background is still wrong; the user meant dithering on the whole area with smooth transitions | `ui/backdrop.rs` draws one continuous surface. A field of three slow waves gives a density for each 24-point block, which is one period of the Bayer matrix. The block is painted with a cached sprite of that period at that density (`dither::level_sprite`). Neighboring blocks differ by a dot or two, so the surface has no edges, and a step costs one sprite per block. The color is fixed per mood so that its sprites are baked once; a change of mood cross-fades two surfaces. |
| The meter tab has sharp corners | The slant curves out of the composer's edge at its foot and into the top edge at its head (`paint_meter_tab`). |
| The model chip and the toolbar order | The chip shows the provider's logo, the account's color square, the model and a chevron. The account's name moved to the tooltip. The order is attachments, skills, a gap, model, effort, then the free space, then mode, approvals and send. The dotted section rules are gone. |
| Use the Anthropic logo for Claude models | `assets/icons/claude.svg` is the Anthropic mark from Simple Icons 16.35.0. This reverses the earlier replacement by a neutral drawing and is recorded as an accepted deviation in `providers.md`. |

## Seventh follow-up, 12 October 2026

The user showed a reference for the background: a vertical dithered gradient whose edge is a row of streaks, and asked for that pattern at half the alpha. `ui/backdrop.rs` now draws it. A column of dots starts its band at `edge + drop behind the text + wave - streak`, in rows. The band is a cached sprite one cell wide (`dither::column_sprite`); there are sixty-four of them per color, for the eight columns of the Bayer matrix and the eight rows a band can start on, which keeps every dot on one fixed grid while a band moves a row at a time. The full grid below a band is a second sprite, repeated. The dot colors have half their earlier alpha. The gradient is measured over the part of the chat that the floating composer leaves in view.

## Eighth follow-up, 12 October 2026

The user asked for the gradient to start at the very bottom and to fill three quarters of the height. The band now starts about a quarter of the way down (`Field::of`) and is 176 rows long. Two things had hidden its bottom: an opaque strip behind the floating composer and a fade above it. Both are gone. They existed to hide messages that scrolled beneath the composer, so the conversation list now ends where the composer begins (`COMPOSER_LEAD` in `ui/chat.rs`) and is clipped there. The background renews its own sprite-baking budget, because it steps between renders of the workspace.

## Ninth follow-up, 12 October 2026

The user asked for a history of the sent messages, as in a supplied reference: a column of ticks beside the conversation and a preview card for the tick under the pointer. `ui/history.rs` builds it as a layer over the conversation list. There is one tick for each message of kind `User`. The tick of the turn in view is wider and takes the text color; the tick under the pointer takes the accent, and the two ticks on each side of it widen less. The preview is a card with a block header (the message's number and age), the sent text, the opening of the turn's last reply and the paragraph after it as a quotation with a dithered rule. Each section is clamped to two lines. A click scrolls the list so that the message is at the top (`Shell::open_turn`).

The rail's place follows from the list's last bounds, so the preview needs no measurement: two spacers share the free height in the ratio of the tick's position, which keeps the card beside its tick and inside the conversation. Only the tick under the pointer is stored (`history_hover`, a message sequence number). A conversation with fewer than two sent messages has no rail. When there are more turns than ticks fit, the rail shows those around the turn in view. Turns of pages that are not loaded have no tick.

Not verified: the look, and the ticks over the right edge of a message bubble in a chat narrower than the reading column. Verified by compilation, formatting and Clippy only.

## Tenth follow-up, 12 October 2026

The user asked for a background with less opacity and more movement. In `ui/backdrop.rs` the dot colors have about half of their earlier alpha (`Mood::tone`), after a second, smaller reduction on the same day. The edge moves more in every mood (`Field::of`): the swell along it is about twice as high and faster, a second, longer swell runs against it, the streaks rise and fall further, and the whole gradient breathes a little in every mood. A band that moves a row changes the share of dots at every height, so the swells also make dots come and go across the whole gradient. The quiet moods step every 110 ms instead of every 140 ms. The cost of one step did not change: the same cached sprites are drawn, one band per column.

Verified by compilation, formatting and Clippy only.

## Eleventh follow-up, 12 October 2026

The user asked for fonts that fit better. Prose and titles used the macOS system font beside Geist Mono labels and code. They now use Geist, the sans-serif face of the same family (`theme::SANS`), so both share proportions and letterforms. Six static faces from Geist release 1.7.2 are bundled unchanged: Regular, Medium, SemiBold, Bold, Italic and Bold Italic. Geist Mono is the same as before; its files are identical to those of the release. All text already took its family from `theme::SANS` or `theme::MONO`, so no screen changed.

Not verified: the look, and line lengths in tight rows, because Geist is slightly wider than the system font. Verified by compilation, formatting and Clippy only.

