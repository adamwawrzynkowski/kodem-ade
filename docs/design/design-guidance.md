# Design guidance for implementation

The current visual language is recorded in [dithered interface](dithered-interface.md). It applies the constraints below.

Decision recorded on 9 September 2026: stop image generation and continue directly with native implementation. The generated concepts were rejected. They are not a reference for the product, and another image set is not an implementation prerequisite. Do not generate more unless the user explicitly asks again.

## What the user wants

The first supplied Zuse screenshot is the main reference for workspace organization: persistent project/task navigation, a substantial coding conversation, adjacent changes, and a compact composer. Keep the other supplied screenshots as references for specific interactions such as full diffs, changed-file trees and conversation navigation.

The user explicitly asked for a professional coding tool that is compact, readable, consistent and pleasant to use, with some life and slight three-dimensional depth. The application remains native Rust with GPUI-CE and the planned GPUI Kit compatibility work.

The exact palette, accent, fonts, wordmark and dimensions in the product specification are starting proposals. Rejection of the generated images did not approve those choices. The original reference guides hierarchy and proportions; its low-contrast labels and truncated paths are problems to improve.

## Lessons from the rejected concepts

The user described the results as generic AI-looking designs. The following is our diagnosis of the recurring patterns, translated into implementation constraints; it does not attribute each individual criticism to the user.

| Pattern to avoid | Implementation rule |
| --- | --- |
| Dashboard composition with large headings and decorative status panels | Open into the active coding task. Put its title in a compact tab or breadcrumb. Give conversation, source, diff and terminal the available space. |
| Repeated rounded cards around ordinary content | Use aligned rows, typography, spacing and occasional separators. Add a boundary when it helps distinguish an approval, code block, input or grouped result. |
| Oversized buttons, model selectors and composer | Use compact native controls. Let the composer grow with its draft; keep secondary choices in local menus. Preserve usable pointer targets and keyboard access. |
| Large empty areas created to make a screenshot look clean | Allocate width deliberately and let panes resize. Empty states should explain the next action briefly. A sparse task can remain sparse without decorative filler. |
| Tiny or faint text used to create apparent density | Recover space from padding and redundant metadata first. Use readable body/code sizes, clear secondary text and configurable density. |
| Color washes, glow, glass and gradients used as personality | Start with quiet neutral content surfaces. Use a restrained accent and semantic Git/status colors. Distinguish selected, hovered, focused, pressed and disabled states. |
| Added shadows without a coherent sense of depth | Use shallow edge highlights, contact shadows and inset inputs only where they explain structure or interaction. Keep reading surfaces calm. Avoid deep bevels and shadows around every row. |
| Inconsistent icons, branding, spacing and component shapes | Use one shared token system and icon family across navigation, chat, review, settings and context menus. Reuse native components and state treatments. |
| Generic assistant presentation dominating the workspace | Keep decoration, large avatars and promotional copy out of the coding flow. Make paths, commands, changes, checks and pending decisions easy to scan. |
| Plausible-looking sample code and successful statuses | Drive enabled UI from actual repository, session and check data. Clearly label fixtures used for permitted headless checks; never present them as live state. |

“Alive” should come from responsive interaction: precise hover and focus, an evident pressed state, informative streaming, stable selection, and restrained transitions. It should not require ambient animation or decorative lighting. Respect reduced motion.

## Apply the rules to the core workflow

Navigation uses a stable project/task hierarchy, a readable wordmark, compact default rows and optional metadata. Sorting, appearance and density belong in a small local settings surface. Resize and persist widths. Long names must remain discoverable without reducing font size. Selecting a task does not check out a branch.

Conversation uses native formatted text, selectable code, compact tool groups and clear pending questions. Maintain a comfortable prose width without forcing code blocks and tables into the same narrow column. Keep the composer attached to its task and preserve both draft and reading position.

Review gives filenames, paths and code enough width to read. Unified and split views share the same visual language. The feedback gutter is visibly separate from text selection and staging. A selected range gets a precise marker and a compact contextual action; writing feedback should retain the surrounding diff. The selection's revision and stale state must remain understandable.

Skills, MCP, provider and worktree controls open near their point of use. Detailed management can have a dedicated view, but a connection inventory should not displace the active coding task by default. Surface provenance, capability limits and errors using the same row and text hierarchy as the rest of the app.

## Implementation and review gates

Build the shell and each working feature in native Rust components, starting with the dependency composition in Phase 0. Do not add another mockup phase. Keep tokens centralized and separate compact/comfortable density from font scaling.

For source and permitted headless checks, cover long paths, a large file tree, lengthy Markdown, tables, code blocks, wrapped diff lines, pending feedback, failures and unavailable capabilities. Check token consistency, text contrast calculations, bounds, overflow policy, keyboard action wiring and real state ownership. Avoid optimizing solely for a short, attractive demonstration task.

Compile every code milestone. Current working agreements forbid app control, screenshots and visual testing, so actual appearance, pointer behavior, text shaping, accessibility and scrolling remain pending for user-performed validation or a later change to that restriction. Source checks and compilation do not establish that the design feels right. Remove temporary test/probe material after use.

Carry these constraints into every phase of the [implementation plan](implementation-plan.md) and the [build prompt](build-prompt.md). Keep the accepted research and functional scope; advance through the actual repository → chat → diff → feedback → recovery workflow.
