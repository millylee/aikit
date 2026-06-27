# aikit

`aikit` is a Rust TUI for managing local AI provider settings and switching the active provider, API key, and model used by AI coding tools.

It is built with [Ratatui](https://ratatui.rs/) and currently targets OpenAI-compatible providers.

## Features

- Manage multiple AI providers with `base_url` and multiple API keys.
- Cache provider model lists locally and refresh them only on demand.
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

## License

BSD-2-Clause
