# Aikit AI Provider TUI Design

## Overview

Build `aikit`, a Rust TUI tool for managing local AI provider configurations and switching the active provider, API key, and model used by common AI coding tools. The first version targets OpenAI-compatible providers, local model caching, and configuration writes for Claude Code, Gemini CLI, and Codex.

The project starts from an empty workspace and should be structured as a release-ready Rust project with CI, cross-platform builds, release artifacts, and one-command install scripts for macOS, Linux, and Windows.

## Goals

- Manage multiple AI providers locally.
- Store each provider's base URL, multiple API keys, and cached model list.
- Allow any API key and model under the same provider to be selected independently.
- Refresh a provider's model list only when the user explicitly requests it.
- Keep using cached models until the user refreshes them again.
- Apply one global active selection to enabled external tools.
- Write the active selection to Claude Code, Gemini CLI, and Codex configs.
- Back up existing target configs before writing.
- Provide a three-column TUI built with Ratatui.
- Provide GitHub workflows for validation, packaging, and release.
- Provide `install.sh` and `install.ps1` for one-command installation from GitHub Releases.

## Non-Goals

- Native Anthropic, Gemini, or other provider-specific model APIs in the first version.
- Automatic background model refresh.
- OS keyring or encrypted secret storage in the first version.
- Per-target active selections in the first version.
- Homebrew, Scoop, or package-manager publishing in the first version.

## Architecture

Use a small Rust workspace with clear boundaries:

- `aikit-core`: domain types, config persistence, provider client, model cache logic, target writer traits, and concrete target writers.
- `aikit-tui`: Ratatui application, layout, event handling, forms, status display, and calls into `aikit-core`.
- Repository root: GitHub workflows, release configuration, install scripts, and project metadata.

The TUI owns interaction state only. Provider management, cache updates, config reads/writes, and external tool integration live in the core layer so they can be tested without terminal UI concerns.

## TUI Design

Use a three-column dashboard:

- Left column: provider list, cache state, add/edit/delete provider actions, and refresh model action.
- Middle column: selected provider details, API key list, model list, and selected key/model state.
- Right column: enabled target tools, detected config paths, last write status, and apply action.

Primary interaction flow:

1. Select a provider.
2. Select an API key under that provider.
3. Select a model from the provider's cached model list.
4. Apply the active selection.
5. Show per-target write status in the right column.

Suggested key bindings:

- `Tab`: move focus between columns.
- Arrow keys: move within the focused list.
- `Enter`: select or edit the focused item.
- `a`: add provider or API key in the current context.
- `e`: edit the focused item.
- `d`: delete the focused item after confirmation.
- `r`: refresh models for the selected provider.
- `Space`: enable or disable a target tool.
- `Ctrl+s`: apply the active selection to enabled targets.
- `q` or `Esc`: quit or back out of a modal.

## Local Configuration

Store user configuration as TOML at `~/.aikit/config.toml` on all platforms. The first version uses a local file with plain API keys and best-effort owner-only permissions.

The configuration contains:

- `providers`: provider ID, display name, base URL, enabled flag, API keys, and selected values.
- `api_keys`: named keys scoped to a provider.
- `models_cache`: cached models scoped to a provider, refresh timestamp, model count, and last error.
- `active_selection`: global provider ID, API key ID, and model ID.
- `targets`: target enablement and detected or configured config path.
- `backup_history`: backup path, target name, write timestamp, and write result metadata.

API key and model selection are independent under a provider. The first version does not require named profiles or bound key/model pairs.

## Provider Query And Model Cache

The first version supports OpenAI-compatible model listing. Refreshing models sends an authenticated request to the selected provider's models endpoint:

```text
GET {base_url}/models
Authorization: Bearer <api_key>
```

The application should normalize base URLs so a user-provided host with or without `/v1` can produce a correct models URL without duplicate path segments.

Cache behavior:

- Refresh only when the user explicitly invokes refresh.
- On success, replace the provider's cached models and record `refreshed_at`.
- On failure, keep the old cache and record `last_error`.
- If no cache exists, the TUI asks the user to refresh before selecting a model.
- Switching models reads only from local cache.

Error classification:

- `401` or `403`: API key or permission problem.
- `404`: base URL may not expose an OpenAI-compatible models endpoint.
- Timeout or connection failure: network problem.
- Invalid response shape: provider compatibility problem.

## External Tool Config Writes

Claude Code, Gemini CLI, and Codex each get an independent target writer. A target writer is responsible for:

- Detecting the default config path.
- Reading existing config.
- Creating a timestamped backup before any write.
- Writing the active `base_url`, API key, and model.
- Returning success, skipped, or failed status for the TUI.

If a target config file does not exist, the writer may create a minimal supported config. If the existing file exists but cannot be parsed safely, the writer must refuse to write and report a clear error. The first version should avoid fragile string replacement in unknown formats.

Successful writes do not require all targets to succeed. Each target reports its own result. Backups and errors are recorded in local config history.

## Testing Strategy

Prioritize tests for core logic:

- Config read/write and default path resolution.
- Owner-only permission best effort where supported.
- OpenAI-compatible model response parsing.
- Provider refresh success and error classification.
- Cache preservation after refresh failure.
- Target writer path detection.
- Backup creation before writes.
- Minimal config creation for missing target files.
- Refusal to write unparseable target configs.
- TUI state transitions for selection, refresh result handling, and apply result display.

TUI rendering does not need broad snapshot coverage in the first version. Keep UI tests focused on state transitions and behavior.

## Git Workflow, CI, And Release

Configure GitHub workflows for:

- Formatting check with `cargo fmt --check`.
- Linting with `cargo clippy`.
- Tests with `cargo test`.
- Release builds on Linux, macOS, and Windows.
- Tag-triggered release artifacts.
- Checksums for release archives.

The binary name is `aikit`. Release artifacts should include platform-specific binaries and the install scripts:

- `install.sh` for macOS and Linux.
- `install.ps1` for Windows.

The install scripts detect platform and architecture, download the matching GitHub Release artifact, install the binary into a user-writable directory, and print PATH guidance when needed.

## Open Questions For Implementation

- Exact Claude Code, Gemini CLI, and Codex config paths and field names should be verified during implementation before writer behavior is finalized. Writers must fail closed if a current format cannot be parsed safely.
- The GitHub repository owner/name is `millylee/aikit`; install scripts should target that repository by default.
