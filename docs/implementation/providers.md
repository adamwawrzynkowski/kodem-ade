# Provider integration

The host binds every conversation to a provider before starting its owner. Old task records and saved preferences default to Codex. A provider cannot change after session startup or while retained outgoing input exists. Starting another conversation preserves the original provider history.

## Caller and ownership

The desktop sends `ConnectRuntime { provider }` to inspect an integration and `SetAgentPreferences` to persist choices before a task exists. `CreateTask` stores the complete selection; subsequent model and permission edits apply to that task. Runtime catalogs, account usage and saved model choices are keyed by `Provider`.

`kodem-host/src/provider.rs` routes operations and owns submission acceptance, the task owner, queue claims, attachment validation and terminal-turn persistence. `providers/codex.rs` implements app-server operations. `providers/devin.rs` and `devin_transport.rs` implement ACP. `kodem-core/src/providers/codex.rs` contains the low-level Codex CLI transport and its wire permission mappings.

The design compared a universal typed session-event interface with native provider drivers. The native drivers were selected because ACP prompt responses arrive at completion while Codex acknowledges a turn before completion. A universal interface would require migrating the existing event decoder without removing those lifecycle differences. Shared queue claiming, uncertainty handling and review capture were retained from the other candidate to keep delivery rules consistent.

## Capabilities and persistence

Settings stores both the last selected provider and each provider's last model, reasoning and access. Delayed window-layout saves preserve these host-owned preferences. The model picker, composer and sidebar use the same provider identity and logo. Model changes clear old context occupancy.

Codex reasoning levels and image modalities come from `model/list`. Devin models, image capability and permission modes come from ACP session configuration. The usage panel also shows session cost when ACP supplies an amount and currency; unavailable account quotas remain explicitly unavailable. Unsupported choices are rejected at host boundaries as well as disabled in the desktop. ACP native slash commands come from its advertised command inventory. The app's `/provider` and `/model` actions open local pickers.

Each provider owns its CLI authentication. Settings supplies its sign-in and sign-out commands for Terminal. Credentials are neither copied into workspace settings nor shared across providers.

Devin runs `session/prompt` asynchronously, so cancellation and permission responses remain available while it works. A transport failure retains uncertain delivery and never silently retries a prompt. ACP does not provide the exact client submission receipt required by Kodem's existing reconciliation policy. Such a task remains uncertain and directs the user to inspect the native session.

## Installed CLI evidence

On 12 September 2026, Devin CLI 3000.10.21 advertised ACP protocol 1, session loading/deletion, image prompts and embedded context. Its modes included Code, Smart, Ask, Plan and Bypass Permissions. The authenticated ACP session exposed only SWE-1.6 Slow and rejected other model IDs even though `devin models list --format json` returned a broader catalog. Kodem uses the runnable ACP catalog. It does not invent reasoning levels or model support from the separate listing.

The probes initialized the CLI and created empty temporary sessions, which were deleted afterward. They sent no model prompts. The installed Codex schema confirmed reasoning options and image modalities, including the schema default for omitted modalities.

Protocol references: [Devin CLI commands](https://docs.devin.ai/cli/reference/commands), [ACP session setup](https://agentclientprotocol.com/protocol/v1/session-setup).

## Verification

Seven temporary persistence and migration regression checks passed, including old host protocol negotiation and retention of the legacy provider selection. Two additional temporary ACP fixture tests passed, covering model configuration, grouped options, asynchronous permissions and cancellation, and UTF-8 output limits. All temporary tests and fixtures were removed. Core, host and desktop compilation passed. Workspace Clippy passed with the pre-existing needless-borrow and collapsible-if lint categories allowed; the strict run also identified and prompted a fix for the new slash-context type-complexity warning. The dependency license snapshot was refreshed and verified for 385 packages. The debug app was packaged and its signature verified. Apple icon conversion required packaging outside the sandbox. No application launch, screenshot, visual test or authenticated model turn is part of this change. Appearance and live model execution require later user validation.
