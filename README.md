# aikit

`aikit` is a Rust TUI for managing local AI provider settings and switching the active provider, API key, and model used by AI coding tools.

It is built with [Ratatui](https://ratatui.rs/) and currently targets OpenAI-compatible providers.

## Features

- Manage multiple AI providers with `base_url` and multiple API keys.
- Cache provider model lists locally, refresh them only on demand, and add manual models for proxy services.
- Select a global provider + API key + model combination.
- Apply the active selection to Claude Code, Gemini CLI, and Codex configs.
- Back up existing target config files before writing.

## Install

macOS/Linux (Recommended):

```sh
curl -fsSL https://github.com/millylee/aikit/releases/latest/download/install.sh | bash
```

Windows (Recommended):

```powershell
irm https://github.com/millylee/aikit/releases/latest/download/install.ps1 | iex
```

## Usage

Start the TUI:

```sh
aikit
```

Print the installed version:

```sh
aikit --version
```

If the installer just added aikit to your PATH, restart the terminal or run the PATH command printed by the installer before using `aikit`.

Installers place the `aikit` binary in `~/.local/bin` by default. Set `AIKIT_BIN_DIR` before running an installer to choose another directory.

### Manage Providers and Keys

The TUI manages providers and API keys directly in `~/.aikit/config.toml`.

Default config location on all platforms:

- `~/.aikit/config.toml`

Runtime state is kept outside the main config:

- `~/.aikit/state.toml`: import prompt state.
- `~/.aikit/cache/models.json`: provider model cache.
- `~/.aikit/backups/<target>/`: centralized backups for `aikit`, Claude Code, Gemini CLI, and Codex configs.
- `~/.aikit/logs/backups.jsonl`: append-only backup index.

Provider management keys in the main TUI:

- `a`: add provider.
- `e`: edit selected provider.
- `d`: delete selected provider (with confirmation).
- `+`: add API key to selected provider. The form accepts an optional display name and a required key value; `aikit` generates the internal key id.
- `x`: delete selected API key (with confirmation).
- `m`: manually add a model to the selected provider.

Import keys and behavior:

- `i`: scan and import provider candidates.
- On startup, if no providers are configured, `aikit` scans environment variables plus Claude Code, Gemini CLI, and Codex config files for import candidates.
- If import candidates are found, `aikit` shows an import prompt before changing config.
- In the prompt, you can import all, skip, or open the selectable candidate list.
- Missing or unparseable config files are soft-failed and shown as warnings; other import sources continue.

Supported environment variables for import:

- OpenAI: `OPENAI_API_KEY`, `OPENAI_BASE_URL`, `OPENAI_MODEL`
- Anthropic: `ANTHROPIC_API_KEY`, `ANTHROPIC_BASE_URL`, `ANTHROPIC_MODEL`
- Gemini: `GEMINI_API_KEY`, `GEMINI_BASE_URL`, `GEMINI_MODEL`

Security note: imported API keys are saved in local TOML as plain text. Keep your machine and config directory protected.

### TUI Keys

- `Tab` or `Right` / `l`: switch to the next pane.
- `Left` / `h`: switch to the previous pane.
- `Up` / `Down` or `k` / `j`: move selection in the focused pane.
- `g` / `G`: jump to the first or last item in the focused pane.
- `Enter` / `Space`: activate the selected provider, API key, model, or toggle the selected target.
- `e`: edit the current provider in Providers, edit the current key/model in Selection, and show a hint in Apply To.
- `r`: refresh models for the selected provider using the selected API key.
- `m`: add a manual model for the selected provider.
- `?`: show shortcuts.
- `u`: check GitHub Releases for updates.
- `Ctrl+s`: apply the active provider + API key + model to enabled targets.
- `q` / `Esc`: quit.

The footer shows the current `aikit` version plus shortcut hints.

Modal form keys:

- `Tab` / `Shift+Tab`: switch input fields.
- `Left` / `Right`: move within the current input.
- `Home` / `End`: jump to the start or end of the current input.
- `Backspace` / `Delete`: delete before or at the current input position.
- `Ctrl+U`: clear the current input.
- `Enter`: save the form.
- `Esc`: cancel the form.

## Development

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Run the TUI locally:

```sh
cargo run -p aikit-tui
```

## Release Version

Bump the workspace version, create a release commit, and tag it:

```powershell
pwsh scripts/version.ps1 patch
```

Push the current branch and tag:

```powershell
pwsh scripts/version.ps1 patch -Push
```

Use `minor`, `major`, or an explicit version when needed:

```powershell
pwsh scripts/version.ps1 minor
pwsh scripts/version.ps1 -Version 0.2.0
```

## License

BSD-2-Clause
