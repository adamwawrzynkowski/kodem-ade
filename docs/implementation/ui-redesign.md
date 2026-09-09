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
