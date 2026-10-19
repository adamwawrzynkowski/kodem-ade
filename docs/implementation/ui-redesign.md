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
| Thread feed | Every open task lives in its project's group, the only layout of a project workspace. A group has no border, only a faint surface in the project's tint under its header and rows, rounded off over a shade of dots: its header shows the project's tile over a dithered glow in the project's tint, its name, a dotted rule, counts of tasks that need input, run or have unread results, and a fold chevron. The glow runs on down the group's rows as a dithered spine that thins out under the last row. Inside a group, tasks that need a person lead: Needs you, Working, Ready for review, then the rest. A folded group whose tasks run keeps their motion on its header. Chats have no project, so their feed keeps the status sections. Settled tasks are not grouped by project: one foldable Settled list stays at the bottom of the sidebar, above the account, and each settled row leads with its project's tile and names the project in its tint. A project group appears only while it has an open task. A finished result is unread until its task is opened. Rows show their state in the leading column, the provider only when it differs from the default or uses another account, the branch and a short diffstat. The footer shows the composer account's limit windows, or its usage credits when the account has no windows. |
| Command palette | Command-K lists every action, thread and account in one ranked list. |
| Keyboard | Next and previous task, Command-1 to Command-9, the next task that needs the user, the agent menus, Shift-Tab to cycle access, Control-backtick for the terminal, and Command-Return to send the other way during a turn. |
| Composer | One chip for provider, model and account, one for effort and one for access. The send button offers stop on an empty draft during a turn and steer or queue on a draft. The agent's step list is a progress strip. `@` completes files of the task's checkout. |
| Agent menu | Provider rail, the provider's accounts with plan and limit meter, models, and a control that makes the current choice the project's default. |
| Requests | Permission requests, plan reviews and questions render inside the composer. A decline can carry feedback. Questions support several choices. The separate approval dialog is removed. |
| Transcript | A run of tool calls has one summary line with counts, totals and a live line for the running call. A thought is not a row of a run: it has a tray of its own and ends the run before it. A run or a thought is open while a call in it runs, or while it ends the transcript of a working conversation; after that it folds (`Shell::tray_open`). A choice made by hand is kept in `tray_choices` and wins. The calls of an open run stay one line each. Failed calls stay visible while the run is folded. Structured calls show a category, a subject, a duration, and an inline diff for edits. One-line notices read as dividers. |
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


## Twelfth follow-up, 12 October 2026

The user asked for four things in two requests.

Masks and task rows. The scanner band of the activity strip showed dots outside the composer's rounded leading corner. The dither painters now take one radius for each corner (`Dither::corner_radii`), and the scanner follows the inside of that corner when the strip is the composer's first row. A task card in the sidebar leads with a 30-point project tile (36 points at first; the user found that a little too big) that spans its two lines (`sidebar::project_tile`), so tasks of different projects are easy to tell apart in a flat list. The tile always shows the project; the state marker of the task moved in front of its title.

Text on dithered surfaces. Cards, message bubbles and history entries shaded a share of their height with dots, so a tall surface had dots behind its text. They now use `dither::edge_shade`, a band of 16 points along the bottom edge. The grain of a selected row (`dither::row_wash`) has a fixed width of 64 points instead of 84% of the row. The chat background did not change.

Arriving text. Only an incomplete reply of the agent formed out of dither. `Shell::message_arriving` now decides this for every message of an agent: an incomplete message of a running thread, or a complete one in the first 1.5 seconds after its creation. Thoughts and tool output use it too. `StreamingText::append` queues up to 576 letters and starts them one after another, so a text that comes whole forms in a sweep and no more than 96 letters form at the same time.

Fonts. Fira Sans 4.203 replaces Geist and IBM Plex Mono 2.3 replaces Geist Mono (`theme::SANS`, `theme::MONO`). The two faces are of different designs. The static files come unchanged from the Google Fonts repository; `assets/THIRD_PARTY.md` records the revisions. Which elements use the mono face did not change.

Not verified: the look of any of these changes. Verified by compilation, formatting and Clippy, and by a temporary test of the timing of the sweep.

## Thirteenth follow-up, 12 October 2026

The user asked for the next messages to live in the prompt field. The tray was a card of its own above the composer, so the tab of the context and usage meters stood in the gap between the two. `Shell::outgoing_tray` is now a row of the composer, between the question panel and the draft context, and the meter tab is always the top of the whole surface. The header of the row keeps its place. Its messages scroll in a list of at most 128 points (`OUTBOX_HEIGHT`, earlier 220 for the whole card), so a long queue or an opened message does not move the draft or its toolbar.

Not verified: the look. Verified by compilation, formatting and Clippy only.

## Fourteenth follow-up, 12 October 2026

The user asked for a parent over the runs of commands, which folds all of them when they are finished, and gave a screenshot of MonoCode as the model. A long turn was a list of one-line trays: a run, a thought, a run, a thought. Runs and thoughts that follow one another without another message between them are now one stretch of work (`tool_calls::work_rows`). A stretch of two or more trays has a parent tray at its first row (`Shell::work_header`): it counts the calls by kind, the thoughts and the edited files, and it shows the edit totals, the failures and the summed duration. The parent follows the same rule as the other trays (`Shell::tray_open`): open while a call in it runs or while it ends the transcript of a working conversation, folded afterwards, and a choice by hand holds. The trays under it keep their own state and are drawn after a rail. A failed call stays visible under a folded parent. A stretch of one tray has no parent.

The transcript list still has one row for each message. Each row draws its own tray and its part of the rail, and a folded parent leaves the rows under it empty, so a long stretch is not one tall list item. A change of a tool message now measures every row of its stretch again (`tool_rows_end`).

A reply of the agent between two tools ends a stretch, so commentary is never folded away.

Not verified: the look. Verified by compilation, formatting and Clippy only.

## Fifteenth follow-up, 12 October 2026

The user asked for many more and better icons in the project configuration, and showed that the book and box tiles did not fit next to tiles that show an application icon.

Tiles. `sidebar::glyph_tile` was a flat square in 8% of the tint with a tinted edge and a tinted outline glyph. It is now a dark plate with the corner radius of an application icon (27% of its side), in both appearances. The tint of the project colors the plate, its edge and a still `Glow` of one-point dots in the leading top corner. The glyph is paper white (`TILE_PAPER`). The tint of the light appearance is raised to a lightness of 0.66 for the edge and the glow, because the plate stays dark. Image tiles take the same radius. Review and chat tiles use the same plate, so one list has one kind of tile. The glyph of a task tile is 18 points (earlier 17).

Icons. `kodem_core::project_icons` holds one table of 225 built-in glyphs in nine groups: Code, Systems, Apps, Business, Creative, Games, Science, World and Objects. `with_project_icons!` gives the table to `ProjectIcon` (variants, labels, assets and `GROUPS`) and to the desktop's asset source, which embeds `assets/icons/projects/<asset>.svg` for each row. A row without its file does not compile. The seven earlier variants keep their names, so saved projects load unchanged; their glyphs are redrawn (Book is an open book, Cube is a box). The vectors are Lucide 1.55.0 with a stroke of 2 and are separate from the interface icons in `assets/icons/`, which keep a stroke of 1.7. A project that follows the automatic choice keeps its kind of glyph (`AUTOMATIC_GLYPHS`).

Picker. The project editor shows the tile at the size of a task row next to the name. The icon section has the automatic choice, the image choice and a search field in one row, and under them a list of at most 236 points with the groups and their counts (`Shell::icon_picker`). A cell is a 28-point glyph button with its name in a tooltip; the chosen cell carries the tint of the project. The search field matches the name of an icon or of a group.

Not verified: the look in the application. A static drawing of the tile was compared with Kodem's icon before the code was written. Verified by compilation, formatting and Clippy, and by temporary tests: the saved forms of the earlier icons still load, and GPUI's renderer draws each of the 225 glyphs.

## Setup dialog, 13 October 2026

The user asked for an onboarding that holds the Artifacts switch, other useful settings and the configuration of the providers, and that finds what is already configured.

`ui/onboarding.rs` builds the dialog `Panel::Setup`, "Welcome to Kodem". It opens once by itself when the first workspace arrives and `Settings.onboarded` is false. The flag is saved when the dialog opens and not when it closes, so each way to close the dialog is final. An installation that has saved settings from before this change has no flag, so it sees the dialog once. The command palette opens the dialog again with "Open setup".

Providers. `Shell::open_setup` sends `ConnectRuntime` for each provider. A row derives its state from `RuntimeInfo` and from the sign-in state of the provider's accounts, and the dialog stores no state of its own: checking, not available with the host's reason, sign-in needed, installed, or ready. A provider that needs a sign-in has a Sign in button for the account that its new tasks use. A provider that is ready and is not the provider of new tasks has "Use for new tasks". A provider that did not start has its installation guide when there is one. "Check again" also sends `RefreshAccount` for each account that does not wait for a sign-in. While the dialog is open, a failed `ConnectRuntime` does not show the error banner, because the row of that provider shows the reason.

Settings. The dialog shows the same rows as Settings and each row writes the same setting: the Claude Code features (`Shell::claude_feature_settings`), Appearance, Default task location and Task notifications. The three rows moved into `Shell::appearance_setting`, `Shell::task_location_setting` and `Shell::task_notifications_setting`, which both screens call.

Not verified: the look in the application, and each provider state in a running window. Verified by compilation, formatting and Clippy.
