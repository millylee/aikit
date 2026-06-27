# Provider Management And Import Design

## Overview

Extend `aikit` so providers can be managed directly from the TUI and initialized from existing AI-related configuration. The first version adds provider/API key add, edit, and delete flows, plus an import flow that scans environment variables and supported AI tool config files.

This builds on the current architecture:

- `aikit-core` owns configuration, target writers, provider refresh, and import logic.
- `aikit-tui` owns the three-pane TUI, modal forms, confirmation dialogs, and user decisions.

## Goals

- Add providers from the TUI.
- Edit provider `id`, `name`, `base_url`, and `enabled`.
- Delete providers from the TUI with confirmation and config backup.
- Add, edit, and delete API keys for a provider.
- Trigger provider import from the TUI.
- On first startup, scan for import candidates and prompt before importing.
- Import API key, base URL, and model values from environment variables.
- Import API key, base URL, and model values from Claude Code, Gemini CLI, and Codex config files where safely parseable.
- Merge imported candidates into existing `aikit` config without overwriting user customizations or cached models.
- Display imported secrets in masked form in the TUI.

## Non-Goals

- Native provider-specific model APIs beyond the existing OpenAI-compatible refresh.
- Secure keyring storage.
- Automatic background import without user confirmation.
- Full external editor integration.
- Import from arbitrary user-selected config files in the first version.

## Import Sources

The import service scans two source categories.

Environment variables:

- `OPENAI_API_KEY`
- `OPENAI_BASE_URL`
- `OPENAI_MODEL`
- `ANTHROPIC_API_KEY`
- `ANTHROPIC_BASE_URL`
- `ANTHROPIC_MODEL`
- `GEMINI_API_KEY`
- `GEMINI_BASE_URL`
- `GEMINI_MODEL`

Supported config files:

- Claude Code config.
- Gemini CLI config.
- Codex config.

Config file scanners must fail soft. If a file is missing or cannot be safely parsed, the scanner returns a warning and continues with other sources.

## Core Import Model

Add an `aikit-core::import` module with these concepts:

- `ImportSource`: `Env`, `Claude`, `Gemini`, `Codex`.
- `ImportCandidate`: a potential provider/key/model import.
- `ImportPlan`: candidates plus warnings.
- `ImportDecision`: selected candidates and merge strategy.
- `ImportResult`: counts of added/updated providers and keys, active selection changes, and warnings.

An `ImportCandidate` contains:

- `source`
- `provider_id`
- `provider_name`
- `base_url`
- `api_key_name`
- `api_key_value`
- `model`
- `warnings`

Secrets are present in memory for import and persistence, but TUI display must mask them by default.

## Import Flow

Startup flow:

1. Load `~/.aikit/config.toml`.
2. If the config has no providers, scan import sources.
3. If candidates exist, show an import prompt before modifying config.
4. Let the user import all, skip, or open a selectable candidate list.
5. Before applying imports, create a backup of `~/.aikit/config.toml` if it exists.
6. Apply selected candidates, save config, and refresh TUI state.

Manual flow:

- Press `i` from the main TUI to rescan import sources.
- Show the same prompt/list as startup.
- Skipping a manual import only dismisses the current prompt; it does not disable future scans.

Startup import must not repeatedly nag after the user skips. Store a lightweight import prompt state in config as a source fingerprint. The prompt reappears only when the newly scanned candidate fingerprint differs from the skipped fingerprint.

## Merge Rules

Import must be conservative:

- Prefer dedupe by `base_url`.
- If `base_url` is unavailable, dedupe by `source + provider_id`.
- Existing provider `name`, `enabled`, and `models_cache` are preserved.
- Missing provider fields may be filled from candidates.
- API keys are deduped by `id` under a provider.
- If a candidate key has the same secret value as an existing key but a different ID, keep the existing key and report the candidate as duplicate.
- If a candidate contains a model and the provider has no cached models, set or update `active_selection` but do not fabricate `models_cache`.
- If a candidate lacks `base_url`, use a known default only when the source is unambiguous; otherwise require edit before import.

## Provider Management UI

Keep the existing three-pane layout. Add modal state for provider/key management and import confirmation.

Provider shortcuts:

- `a`: add provider.
- `e`: edit selected provider.
- `d`: delete selected provider after confirmation.
- `i`: scan and import candidates.
- `k`: add API key to selected provider.
- `x`: delete selected API key after confirmation.
- `r`: refresh models for selected provider.
- `Enter`: activate selected provider/key/model.
- `Ctrl+s`: apply active selection to enabled targets.

Provider modal fields:

- `id`
- `name`
- `base_url`
- `enabled`

API key modal fields:

- `id`
- `name`
- `value`

Modal controls:

- `Tab`: next field.
- `Shift+Tab`: previous field.
- `Enter`: save.
- `Esc`: cancel.

Validation:

- Provider ID is required and unique.
- API key ID is required and unique within the provider.
- `base_url` is required and must parse as a URL.
- API key value is required for new keys.

API key values are masked when displayed outside edit mode.

## Delete And Backup Behavior

Before deleting a provider or API key:

1. Show a confirmation dialog.
2. Backup `~/.aikit/config.toml` if it exists.
3. Apply the deletion.
4. Save config.
5. Refresh TUI state.

Deleting a provider removes its API keys and model cache. If `active_selection` points to the deleted provider, clear `active_selection`.

Deleting an API key clears `active_selection` if it points to the deleted key. The TUI must show a status message requiring the user to select another key/model before applying targets again.

## Error Handling

- Import scanner warnings are shown in the import modal and status line.
- A failed scanner does not block other scanners.
- A failed config backup blocks destructive operations.
- A failed save keeps the in-memory state unchanged and reports the error.
- Invalid modal input stays in the modal and shows field-level error text.

## Testing Strategy

Core tests:

- Environment variable scan produces candidates for key/base URL/model combinations.
- Config file scanners parse supported structures and fail soft on invalid files.
- Import merge preserves provider name, enabled state, and model cache.
- Import merge dedupes keys by ID and by secret value.
- Import with model but no cache updates active selection without creating fake cache.
- Provider delete backs up config and clears active selection.
- API key delete backs up config and clears affected active selection.

TUI tests:

- Provider modal validates required fields and duplicate IDs.
- Provider modal save adds and edits providers.
- API key modal save adds and edits keys.
- Delete confirmation cancels or applies correctly.
- Import prompt appears when candidates exist and providers are empty.
- Import skip does not write config.
- Import confirmation writes config and refreshes visible state.

## Documentation Updates

Update README usage after implementation:

- Describe adding/editing/deleting providers from the TUI.
- Describe `i` import flow.
- List supported environment variables.
- Explain that imported API keys are saved in local TOML as plain text.
