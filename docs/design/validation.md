# Validation and evidence

Historical research/design validation performed on 9 September 2026, before application implementation. The current application evidence is recorded in [implementation and validation](../implementation/progress.md). The results below describe the earlier probe only.

## Native compilation

Host: Apple Silicon arm64, macOS 27.0. Toolchain: rustc 1.97.1 and cargo 1.97.1. The active developer tools path was `/Library/Developer/CommandLineTools`. No global toolchain or Xcode selection was changed.

A temporary Rust executable depended on these exact published versions:

```toml
[dependencies]
gpui = { package = "gpui-ce", version = "=0.2.2", default-features = false, features = ["font-kit"] }
gpui_platform = { package = "gpui_ce_platform", version = "=0.1.0", features = ["font-kit"] }
```

Its native render implementation constructed a GPUI div, color and text. The entry point called `gpui_platform::application()`, `run`, `open_window` and `cx.new`. The binary was built and linked, never executed.

Command: `cargo build --manifest-path <temporary-probe>/Cargo.toml`.

Result: exit 0. Cargo reported `Finished dev profile [unoptimized] target(s) in 1m 11s`. The native binary existed. This is evidence for a native CE dependency path on this machine, not a release benchmark or application test.

Cargo also warned that transitive `block 0.1.6` contains code rejected by a future Rust version. Investigate during dependency pinning. The current build passed.

The tested packages were published releases. They are not identical evidence to compiling the separately inspected upstream main commits. Both release and main manifest observations are labelled in [research](research.md).

## GPUI Kit interoperability

The same temporary project then added `gpui-kit = "=0.6.0"` and compiled a separate check-only binary:

```rust
use gpui::IntoElement;

fn needs_ce_element(_value: impl IntoElement) {}

/// Checks the unmodified Kit component against the CE render contract.
fn main() {
    needs_ce_element(gpui_kit::component::button::Button::new("probe"));
}
```

Command: `cargo check --manifest-path <temporary-probe>/Cargo.toml --bin kit-seam`.

Result: exit 101, E0277.

```text
the trait bound
gpui_kit::gpui_component::button::Button: gpui::IntoElement
is not satisfied

note: there are multiple different versions of crate gpui
in the dependency graph
```

The compiler identified GPUI-CE 0.2.2's IntoElement as the expected trait and gpui-pre 0.3.4's IntoElement as the other trait. `cargo tree -i gpui-pre --depth 2` showed Kit, base, component, assets and platform packages using gpui-pre. The Kit manifest's 0.3.1 requirement allowed resolution to 0.3.4.

This is a concrete failed interoperability check, not a failure of the successful CE-only native build. The project must port/pin a compatible component set or make an explicit stack decision before depending on Kit controls.

Final mixed-probe Cargo.lock SHA-256: `21209f9377cbe465d598a408f3ae1b7e0dd367a226c8642ab3927bb0b09e77fe`. Native probe source SHA-256: `c985fc31312f70fd4166ef9ec176272938ed17dc94e72bde0f0c48905dfce155`. The lock hash identifies the mixed probe after Kit was added; it is not an application lockfile.

To reproduce later, create a disposable crate with the manifest above and the native entry point described above, build without running, then add Kit and the second snippet. Preserve the resulting resolved dependency versions in the new validation record. Cargo's transitive resolution may change if a lockfile is not retained.

## Research and source checks

- Reviewed all seven named products and five additional tools through primary documentation/repositories.
- Inspected the manifests at fixed GPUI-CE and Kit main revisions and locally downloaded published Cargo manifests.
- Verified installed Codex version `0.153.4` and app-server help without starting the server.
- Checked Pi's repository redirect and the distinction between built-in features and extension-supplied MCP/subagent/permission support.
- Checked current MCP specification routing. At this research date `/specification/latest` redirected to `2026-07-28`. Older provider versions may negotiate older versions. [Current specification](https://modelcontextprotocol.io/specification/2026-07-28)
- Read the computer-history skill at `/Users/adamwaw/.codex/plugins/cache/openai-bundled/computer-history/1.0.1000926/skills/computer-history/SKILL.md`. Its required `computer_history_status` tool was unavailable. No activity-history claims were made from unverified capture data.
- Historical memory informed the distinction between cached and available extensions and the need to keep branch activation explicit. Live source checks establish the technical findings in this package.

## Design scope correction

The user rejected the generated concepts and then explicitly cancelled image generation. The active generation script was terminated. No further image generation is part of this delivery. The generated project images, gallery and generation prompts were removed. Files from this task's generated-image output directory were also removed; the supplied reference attachments were preserved.

[Design guidance](design-guidance.md) records explicit user preferences separately from our diagnosis of the rejected patterns. The product specification, implementation plan and enhanced prompt now use that guidance and the supplied references. Exact visual tokens remain proposals. No generated concept is an approved implementation reference.

## Final audit

The final document audit checks local Markdown links, code-fence balance, whitespace, removed-image references and changed-file scope. Image count and visual inspection are no longer acceptance checks for this delivery.

Result: 11 Markdown files and 23 local links passed the link, fence and whitespace checks. No obsolete gallery/prompt links or generated PNG files remain in the design package. The task's generated-image directory and temporary probe marker are clear. `git diff --check` passed.

All temporary native probe sources, binaries, logs and the temporary path marker were removed after evidence extraction. Cargo's normal package cache remains.

No application launch, control, screenshot, visual test, live provider turn, repository mutation by an agent runtime, commit, or push was performed. The pre-existing `.DS_Store` is preserved.

Interaction quality, accessibility behavior, real provider integration, the Kit compatibility port, platform portability and performance targets remain unverified. These are implementation gates, not completed results. The current correction changed documentation and removed generated artifacts; it did not change Rust source or require a new compilation.
