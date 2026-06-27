# Aikit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `aikit`, a Rust TUI for managing OpenAI-compatible AI providers and applying one active provider/key/model selection to Claude Code, Gemini CLI, and Codex configs.

**Architecture:** Use a Rust workspace with a testable `aikit-core` crate and a thin `aikit-tui` binary crate. Keep provider queries, config persistence, model caching, backups, and target writers out of the TUI layer.

**Tech Stack:** Rust stable, Ratatui, Crossterm, Tokio, Reqwest, Serde, TOML, Directories, Color-eyre, Thiserror, Tempfile, Wiremock, GitHub Actions.

## Global Constraints

- Product name, binary name, config directory, and release artifact name use `aikit`.
- First provider API support is OpenAI-compatible model listing only.
- API keys are stored in local TOML config with best-effort owner-only permissions.
- Model refresh is manual only; switching reads local cache.
- One global active provider/key/model selection is applied to enabled targets.
- Target writers back up existing config before writing.
- Target writers fail closed when an existing target config cannot be parsed safely.
- The first release supports macOS, Linux, and Windows artifacts plus `install.sh` and `install.ps1`.
- Do not add package-manager publishing in the first release.

---

## File Structure

- Create `Cargo.toml`: workspace definition and shared dependency versions.
- Create `rust-toolchain.toml`: stable toolchain pin.
- Create `crates/aikit-core/Cargo.toml`: core library crate.
- Create `crates/aikit-core/src/lib.rs`: public module exports.
- Create `crates/aikit-core/src/error.rs`: shared `AikitError` and `Result`.
- Create `crates/aikit-core/src/config.rs`: config types, path resolution, load/save.
- Create `crates/aikit-core/src/provider.rs`: OpenAI-compatible model client and parser.
- Create `crates/aikit-core/src/cache.rs`: model cache refresh rules.
- Create `crates/aikit-core/src/targets/mod.rs`: target writer trait and shared write result types.
- Create `crates/aikit-core/src/targets/backup.rs`: timestamped backup helpers.
- Create `crates/aikit-core/src/targets/claude.rs`: Claude Code writer.
- Create `crates/aikit-core/src/targets/gemini.rs`: Gemini CLI writer.
- Create `crates/aikit-core/src/targets/codex.rs`: Codex writer.
- Create `crates/aikit-core/tests/config_tests.rs`: config persistence tests.
- Create `crates/aikit-core/tests/provider_tests.rs`: provider parsing and HTTP tests.
- Create `crates/aikit-core/tests/target_tests.rs`: target writer and backup tests.
- Create `crates/aikit-tui/Cargo.toml`: TUI binary crate.
- Create `crates/aikit-tui/src/main.rs`: terminal setup and app entry point.
- Create `crates/aikit-tui/src/app.rs`: app state and core command orchestration.
- Create `crates/aikit-tui/src/ui.rs`: Ratatui rendering.
- Create `crates/aikit-tui/src/input.rs`: key handling.
- Create `crates/aikit-tui/tests/app_state_tests.rs`: TUI state transition tests.
- Create `.github/workflows/check.yml`: fmt, clippy, tests.
- Create `.github/workflows/release.yml`: cross-platform release build.
- Create `scripts/install.sh`: macOS/Linux release installer.
- Create `scripts/install.ps1`: Windows release installer.

## Task 1: Workspace Scaffold

**Files:**
- Create: `Cargo.toml`
- Create: `rust-toolchain.toml`
- Create: `crates/aikit-core/Cargo.toml`
- Create: `crates/aikit-core/src/lib.rs`
- Create: `crates/aikit-core/src/error.rs`
- Create: `crates/aikit-tui/Cargo.toml`
- Create: `crates/aikit-tui/src/main.rs`

**Interfaces:**
- Produces: workspace crates `aikit-core` and binary `aikit`.
- Produces: `pub type Result<T> = std::result::Result<T, AikitError>`.

- [ ] **Step 1: Write the initial workspace files**

`Cargo.toml`:

```toml
[workspace]
members = ["crates/aikit-core", "crates/aikit-tui"]
resolver = "2"

[workspace.package]
edition = "2021"
license = "MIT"
repository = "https://github.com/aikit-rs/aikit"

[workspace.dependencies]
color-eyre = "0.6"
crossterm = "0.29"
directories = "6"
ratatui = "0.30"
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tempfile = "3"
thiserror = "2"
time = { version = "0.3", features = ["formatting", "macros", "serde"] }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
toml = "0.9"
wiremock = "0.6"
```

`rust-toolchain.toml`:

```toml
[toolchain]
channel = "stable"
components = ["rustfmt", "clippy"]
```

`crates/aikit-core/Cargo.toml`:

```toml
[package]
name = "aikit-core"
version = "0.1.0"
edition.workspace = true
license.workspace = true
repository.workspace = true

[dependencies]
directories.workspace = true
reqwest.workspace = true
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
time.workspace = true
toml.workspace = true

[dev-dependencies]
tempfile.workspace = true
tokio.workspace = true
wiremock.workspace = true
```

`crates/aikit-tui/Cargo.toml`:

```toml
[package]
name = "aikit-tui"
version = "0.1.0"
edition.workspace = true
license.workspace = true
repository.workspace = true

[[bin]]
name = "aikit"
path = "src/main.rs"

[dependencies]
aikit-core = { path = "../aikit-core" }
color-eyre.workspace = true
crossterm.workspace = true
ratatui.workspace = true
tokio.workspace = true
```

- [ ] **Step 2: Add minimal library and binary code**

`crates/aikit-core/src/lib.rs`:

```rust
pub mod error;

pub use error::{AikitError, Result};
```

`crates/aikit-core/src/error.rs`:

```rust
use thiserror::Error;

pub type Result<T> = std::result::Result<T, AikitError>;

#[derive(Debug, Error)]
pub enum AikitError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("config parse error: {0}")]
    ConfigParse(String),
    #[error("provider error: {0}")]
    Provider(String),
    #[error("target write error: {0}")]
    TargetWrite(String),
}
```

`crates/aikit-tui/src/main.rs`:

```rust
use color_eyre::Result;

fn main() -> Result<()> {
    color_eyre::install()?;
    println!("aikit");
    Ok(())
}
```

- [ ] **Step 3: Run scaffold checks**

Run: `cargo fmt --check`

Expected: command exits successfully with no formatted diff.

Run: `cargo clippy --workspace --all-targets -- -D warnings`

Expected: command exits successfully with no warnings.

Run: `cargo test --workspace`

Expected: command exits successfully and reports the initial crates compiled.

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml rust-toolchain.toml crates/aikit-core crates/aikit-tui
git commit -m "Scaffold aikit Rust workspace"
```

## Task 2: Config Model And Persistence

**Files:**
- Create: `crates/aikit-core/src/config.rs`
- Modify: `crates/aikit-core/src/lib.rs`
- Test: `crates/aikit-core/tests/config_tests.rs`

**Interfaces:**
- Produces: `AikitConfig`, `ProviderConfig`, `ApiKeyConfig`, `ModelCache`, `ActiveSelection`, `TargetConfig`, `BackupRecord`.
- Produces: `AikitConfig::load_from(path: &Path) -> Result<Self>`.
- Produces: `AikitConfig::save_to(&self, path: &Path) -> Result<()>`.
- Produces: `default_config_path() -> Result<PathBuf>`.

- [ ] **Step 1: Write failing config tests**

`crates/aikit-core/tests/config_tests.rs`:

```rust
use aikit_core::config::{
    AikitConfig, ApiKeyConfig, ModelCache, ProviderConfig, TargetConfig, default_config_path,
};
use tempfile::tempdir;

#[test]
fn saves_and_loads_config_as_toml() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("aikit").join("config.toml");
    let config = AikitConfig {
        providers: vec![ProviderConfig {
            id: "openrouter".into(),
            name: "OpenRouter".into(),
            base_url: "https://openrouter.ai/api/v1".into(),
            enabled: true,
            api_keys: vec![ApiKeyConfig {
                id: "work".into(),
                name: "Work".into(),
                value: "sk-test".into(),
            }],
            models_cache: Some(ModelCache {
                refreshed_at: "2026-06-27T00:00:00Z".into(),
                models: vec!["openai/gpt-4.1-mini".into()],
                last_error: None,
            }),
        }],
        active_selection: None,
        targets: vec![TargetConfig {
            id: "codex".into(),
            enabled: true,
            config_path: None,
        }],
        backup_history: vec![],
    };

    config.save_to(&path).unwrap();
    let loaded = AikitConfig::load_from(&path).unwrap();

    assert_eq!(loaded.providers[0].id, "openrouter");
    assert_eq!(loaded.providers[0].api_keys[0].value, "sk-test");
    assert_eq!(loaded.targets[0].id, "codex");
}

#[test]
fn default_path_ends_with_aikit_config_toml() {
    let path = default_config_path().unwrap();
    assert!(path.ends_with("aikit/config.toml") || path.ends_with("aikit\\config.toml"));
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test -p aikit-core --test config_tests`

Expected: FAIL because `config` module and exported types do not exist.

- [ ] **Step 3: Implement config types and persistence**

`crates/aikit-core/src/lib.rs`:

```rust
pub mod config;
pub mod error;

pub use error::{AikitError, Result};
```

`crates/aikit-core/src/config.rs`:

```rust
use std::{fs, path::{Path, PathBuf}};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::{AikitError, Result};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AikitConfig {
    pub providers: Vec<ProviderConfig>,
    pub active_selection: Option<ActiveSelection>,
    pub targets: Vec<TargetConfig>,
    pub backup_history: Vec<BackupRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderConfig {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub enabled: bool,
    pub api_keys: Vec<ApiKeyConfig>,
    pub models_cache: Option<ModelCache>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApiKeyConfig {
    pub id: String,
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelCache {
    pub refreshed_at: String,
    pub models: Vec<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActiveSelection {
    pub provider_id: String,
    pub api_key_id: String,
    pub model_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TargetConfig {
    pub id: String,
    pub enabled: bool,
    pub config_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackupRecord {
    pub target_id: String,
    pub backup_path: PathBuf,
    pub written_at: String,
    pub status: String,
}

impl Default for AikitConfig {
    fn default() -> Self {
        Self {
            providers: Vec::new(),
            active_selection: None,
            targets: vec![
                TargetConfig { id: "claude".into(), enabled: true, config_path: None },
                TargetConfig { id: "gemini".into(), enabled: true, config_path: None },
                TargetConfig { id: "codex".into(), enabled: true, config_path: None },
            ],
            backup_history: Vec::new(),
        }
    }
}

impl AikitConfig {
    pub fn load_from(path: &Path) -> Result<Self> {
        let data = fs::read_to_string(path)?;
        toml::from_str(&data).map_err(|err| AikitError::ConfigParse(err.to_string()))
    }

    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let data = toml::to_string_pretty(self)
            .map_err(|err| AikitError::ConfigParse(err.to_string()))?;
        fs::write(path, data)?;
        set_owner_only(path)?;
        Ok(())
    }
}

pub fn default_config_path() -> Result<PathBuf> {
    let dirs = ProjectDirs::from("", "", "aikit")
        .ok_or_else(|| AikitError::ConfigParse("could not determine config directory".into()))?;
    Ok(dirs.config_dir().join("config.toml"))
}

#[cfg(unix)]
fn set_owner_only(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o600);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_owner_only(_path: &Path) -> Result<()> {
    Ok(())
}
```

- [ ] **Step 4: Run config tests**

Run: `cargo test -p aikit-core --test config_tests`

Expected: PASS for both config tests.

- [ ] **Step 5: Commit**

```bash
git add crates/aikit-core/src/lib.rs crates/aikit-core/src/config.rs crates/aikit-core/tests/config_tests.rs
git commit -m "Add aikit config persistence"
```

## Task 3: OpenAI-Compatible Provider Client And Cache

**Files:**
- Create: `crates/aikit-core/src/provider.rs`
- Create: `crates/aikit-core/src/cache.rs`
- Modify: `crates/aikit-core/src/lib.rs`
- Modify: `crates/aikit-core/src/error.rs`
- Test: `crates/aikit-core/tests/provider_tests.rs`

**Interfaces:**
- Produces: `OpenAiCompatibleClient::new(reqwest::Client) -> Self`.
- Produces: `OpenAiCompatibleClient::list_models(&self, base_url: &str, api_key: &str) -> Result<Vec<String>>`.
- Produces: `refresh_models(provider: &mut ProviderConfig, api_key_id: &str, client: &OpenAiCompatibleClient) -> Result<()>`.

- [ ] **Step 1: Write failing provider tests**

`crates/aikit-core/tests/provider_tests.rs`:

```rust
use aikit_core::{cache::refresh_models, config::{ApiKeyConfig, ProviderConfig}, provider::OpenAiCompatibleClient};
use wiremock::{Mock, MockServer, ResponseTemplate, matchers::{method, path}};

#[tokio::test]
async fn lists_models_from_openai_compatible_endpoint() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": [
                { "id": "model-a" },
                { "id": "model-b" }
            ]
        })))
        .mount(&server)
        .await;

    let client = OpenAiCompatibleClient::new(reqwest::Client::new());
    let models = client.list_models(&format!("{}/v1", server.uri()), "sk-test").await.unwrap();

    assert_eq!(models, vec!["model-a", "model-b"]);
}

#[tokio::test]
async fn refresh_failure_keeps_existing_cache() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let mut provider = ProviderConfig {
        id: "p".into(),
        name: "Provider".into(),
        base_url: format!("{}/v1", server.uri()),
        enabled: true,
        api_keys: vec![ApiKeyConfig { id: "k".into(), name: "Key".into(), value: "bad".into() }],
        models_cache: Some(aikit_core::config::ModelCache {
            refreshed_at: "old".into(),
            models: vec!["old-model".into()],
            last_error: None,
        }),
    };
    let client = OpenAiCompatibleClient::new(reqwest::Client::new());

    let result = refresh_models(&mut provider, "k", &client).await;

    assert!(result.is_err());
    let cache = provider.models_cache.unwrap();
    assert_eq!(cache.models, vec!["old-model"]);
    assert!(cache.last_error.unwrap().contains("authentication"));
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test -p aikit-core --test provider_tests`

Expected: FAIL because `provider` and `cache` modules do not exist.

- [ ] **Step 3: Implement provider client and refresh logic**

Add `pub mod provider; pub mod cache;` to `crates/aikit-core/src/lib.rs`.

`crates/aikit-core/src/provider.rs` defines response structs:

```rust
use serde::Deserialize;

use crate::{AikitError, Result};

#[derive(Debug, Deserialize)]
struct ModelListResponse {
    data: Vec<ModelItem>,
}

#[derive(Debug, Deserialize)]
struct ModelItem {
    id: String,
}

#[derive(Debug, Clone)]
pub struct OpenAiCompatibleClient {
    http: reqwest::Client,
}

impl OpenAiCompatibleClient {
    pub fn new(http: reqwest::Client) -> Self {
        Self { http }
    }

    pub async fn list_models(&self, base_url: &str, api_key: &str) -> Result<Vec<String>> {
        let url = models_url(base_url);
        let response = self.http.get(url).bearer_auth(api_key).send().await
            .map_err(|err| AikitError::Provider(format!("network error: {err}")))?;
        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(AikitError::Provider("authentication or permission problem".into()));
        }
        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(AikitError::Provider("models endpoint was not found".into()));
        }
        if !status.is_success() {
            return Err(AikitError::Provider(format!("provider returned status {status}")));
        }
        let body: ModelListResponse = response.json().await
            .map_err(|err| AikitError::Provider(format!("invalid model response: {err}")))?;
        Ok(body.data.into_iter().map(|model| model.id).collect())
    }
}

fn models_url(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if trimmed.ends_with("/v1") {
        format!("{trimmed}/models")
    } else {
        format!("{trimmed}/v1/models")
    }
}
```

`crates/aikit-core/src/cache.rs`:

```rust
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{AikitError, Result, config::ModelCache, config::ProviderConfig, provider::OpenAiCompatibleClient};

pub async fn refresh_models(
    provider: &mut ProviderConfig,
    api_key_id: &str,
    client: &OpenAiCompatibleClient,
) -> Result<()> {
    let key = provider.api_keys.iter()
        .find(|key| key.id == api_key_id)
        .ok_or_else(|| AikitError::Provider(format!("api key not found: {api_key_id}")))?;

    match client.list_models(&provider.base_url, &key.value).await {
        Ok(models) => {
            provider.models_cache = Some(ModelCache {
                refreshed_at: OffsetDateTime::now_utc().format(&Rfc3339).unwrap(),
                models,
                last_error: None,
            });
            Ok(())
        }
        Err(err) => {
            if let Some(cache) = provider.models_cache.as_mut() {
                cache.last_error = Some(err.to_string());
            } else {
                provider.models_cache = Some(ModelCache {
                    refreshed_at: String::new(),
                    models: Vec::new(),
                    last_error: Some(err.to_string()),
                });
            }
            Err(err)
        }
    }
}
```

- [ ] **Step 4: Run provider tests**

Run: `cargo test -p aikit-core --test provider_tests`

Expected: PASS for model listing and cache preservation tests.

- [ ] **Step 5: Commit**

```bash
git add crates/aikit-core/src/lib.rs crates/aikit-core/src/error.rs crates/aikit-core/src/provider.rs crates/aikit-core/src/cache.rs crates/aikit-core/tests/provider_tests.rs
git commit -m "Add OpenAI-compatible model refresh"
```

## Task 4: Target Writer Interface, Backups, And Codex Writer

**Files:**
- Create: `crates/aikit-core/src/targets/mod.rs`
- Create: `crates/aikit-core/src/targets/backup.rs`
- Create: `crates/aikit-core/src/targets/codex.rs`
- Modify: `crates/aikit-core/src/lib.rs`
- Test: `crates/aikit-core/tests/target_tests.rs`

**Interfaces:**
- Produces: `TargetSelection { base_url, api_key, model }`.
- Produces: `TargetWriter` trait.
- Produces: `backup_file(path: &Path) -> Result<PathBuf>`.
- Produces: `CodexWriter::write_to_path(path: &Path, selection: &TargetSelection) -> Result<TargetWriteResult>`.

- [ ] **Step 1: Write failing target writer tests**

`crates/aikit-core/tests/target_tests.rs`:

```rust
use aikit_core::targets::{TargetSelection, codex::CodexWriter};
use tempfile::tempdir;

#[test]
fn codex_writer_creates_backup_before_writing_existing_config() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "model = \"old\"\n").unwrap();

    let result = CodexWriter::write_to_path(&path, &TargetSelection {
        base_url: "https://example.com/v1".into(),
        api_key: "sk-new".into(),
        model: "model-new".into(),
    }).unwrap();

    assert!(result.backup_path.unwrap().exists());
    let updated = std::fs::read_to_string(path).unwrap();
    assert!(updated.contains("model-new"));
    assert!(updated.contains("https://example.com/v1"));
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test -p aikit-core --test target_tests`

Expected: FAIL because `targets` module does not exist.

- [ ] **Step 3: Implement target primitives and Codex writer**

`crates/aikit-core/src/lib.rs` adds `pub mod targets;`.

`crates/aikit-core/src/targets/mod.rs`:

```rust
pub mod backup;
pub mod codex;

use std::path::PathBuf;

use crate::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetSelection {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetWriteResult {
    pub target_id: String,
    pub config_path: PathBuf,
    pub backup_path: Option<PathBuf>,
}

pub trait TargetWriter {
    fn target_id(&self) -> &'static str;
    fn default_path(&self) -> Result<PathBuf>;
    fn write(&self, selection: &TargetSelection) -> Result<TargetWriteResult>;
}
```

`crates/aikit-core/src/targets/backup.rs`:

```rust
use std::{fs, path::{Path, PathBuf}};
use time::{OffsetDateTime, format_description::FormatItem, macros::format_description};

use crate::Result;

const BACKUP_FORMAT: &[FormatItem<'_>] = format_description!("[year][month][day]-[hour][minute][second]");

pub fn backup_file(path: &Path) -> Result<Option<PathBuf>> {
    if !path.exists() {
        return Ok(None);
    }
    let timestamp = OffsetDateTime::now_utc().format(BACKUP_FORMAT).unwrap();
    let backup_path = path.with_extension(format!("bak.{timestamp}"));
    fs::copy(path, &backup_path)?;
    Ok(Some(backup_path))
}
```

`crates/aikit-core/src/targets/codex.rs`:

```rust
use std::{fs, path::{Path, PathBuf}};

use directories::BaseDirs;

use crate::{AikitError, Result};
use super::{TargetSelection, TargetWriteResult, TargetWriter, backup::backup_file};

pub struct CodexWriter;

impl CodexWriter {
    pub fn write_to_path(path: &Path, selection: &TargetSelection) -> Result<TargetWriteResult> {
        let backup_path = backup_file(path)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = format!(
            "model = \"{}\"\nmodel_provider = \"aikit\"\n[model_providers.aikit]\nname = \"aikit\"\nbase_url = \"{}\"\nenv_key = \"AIKIT_API_KEY\"\n\n",
            selection.model,
            selection.base_url,
        );
        fs::write(path, content)?;
        Ok(TargetWriteResult {
            target_id: "codex".into(),
            config_path: path.to_path_buf(),
            backup_path,
        })
    }
}

impl TargetWriter for CodexWriter {
    fn target_id(&self) -> &'static str {
        "codex"
    }

    fn default_path(&self) -> Result<PathBuf> {
        let dirs = BaseDirs::new()
            .ok_or_else(|| AikitError::TargetWrite("could not determine home directory".into()))?;
        Ok(dirs.home_dir().join(".codex").join("config.toml"))
    }

    fn write(&self, selection: &TargetSelection) -> Result<TargetWriteResult> {
        Self::write_to_path(&self.default_path()?, selection)
    }
}
```

- [ ] **Step 4: Run target tests**

Run: `cargo test -p aikit-core --test target_tests`

Expected: PASS for Codex backup and write test.

- [ ] **Step 5: Commit**

```bash
git add crates/aikit-core/src/lib.rs crates/aikit-core/src/targets crates/aikit-core/tests/target_tests.rs
git commit -m "Add target writer backup support"
```

## Task 5: Claude Code And Gemini Writers

**Files:**
- Create: `crates/aikit-core/src/targets/claude.rs`
- Create: `crates/aikit-core/src/targets/gemini.rs`
- Modify: `crates/aikit-core/src/targets/mod.rs`
- Modify: `crates/aikit-core/tests/target_tests.rs`

**Interfaces:**
- Produces: `ClaudeWriter::write_to_path(path: &Path, selection: &TargetSelection) -> Result<TargetWriteResult>`.
- Produces: `GeminiWriter::write_to_path(path: &Path, selection: &TargetSelection) -> Result<TargetWriteResult>`.

- [ ] **Step 1: Extend target tests**

Add to `crates/aikit-core/tests/target_tests.rs`:

```rust
use aikit_core::targets::{claude::ClaudeWriter, gemini::GeminiWriter};

#[test]
fn claude_writer_creates_minimal_json_config() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");

    ClaudeWriter::write_to_path(&path, &TargetSelection {
        base_url: "https://example.com/v1".into(),
        api_key: "sk-new".into(),
        model: "claude-model".into(),
    }).unwrap();

    let updated = std::fs::read_to_string(path).unwrap();
    assert!(updated.contains("claude-model"));
    assert!(updated.contains("https://example.com/v1"));
}

#[test]
fn gemini_writer_refuses_invalid_json() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");
    std::fs::write(&path, "{invalid json").unwrap();

    let result = GeminiWriter::write_to_path(&path, &TargetSelection {
        base_url: "https://example.com/v1".into(),
        api_key: "sk-new".into(),
        model: "gemini-model".into(),
    });

    assert!(result.is_err());
    assert!(std::fs::read_to_string(path).unwrap().contains("{invalid json"));
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test -p aikit-core --test target_tests`

Expected: FAIL because Claude and Gemini writers do not exist.

- [ ] **Step 3: Implement JSON target writers**

`crates/aikit-core/src/targets/mod.rs` adds:

```rust
pub mod claude;
pub mod gemini;
```

Implement both writers with the same safe pattern:

```rust
use std::{fs, path::{Path, PathBuf}};

use directories::BaseDirs;
use serde_json::{Value, json};

use crate::{AikitError, Result};
use super::{TargetSelection, TargetWriteResult, TargetWriter, backup::backup_file};
```

For `ClaudeWriter`, default path is `~/.claude/settings.json`. For `GeminiWriter`, default path is `~/.gemini/settings.json`.

Each `write_to_path` loads existing JSON when the file exists:

```rust
let mut value: Value = if path.exists() {
    serde_json::from_str(&fs::read_to_string(path)?)
        .map_err(|err| AikitError::TargetWrite(format!("invalid json config: {err}")))?
} else {
    json!({})
};
```

Then set an `aikit` object while preserving other fields:

```rust
value["aikit"] = json!({
    "base_url": selection.base_url,
    "api_key": selection.api_key,
    "model": selection.model
});
```

Create a backup before writing, pretty-print JSON, and return `TargetWriteResult` with target IDs `claude` and `gemini`.

- [ ] **Step 4: Run target tests**

Run: `cargo test -p aikit-core --test target_tests`

Expected: PASS for Codex, Claude, and Gemini writer tests.

- [ ] **Step 5: Commit**

```bash
git add crates/aikit-core/src/targets crates/aikit-core/tests/target_tests.rs
git commit -m "Add Claude and Gemini target writers"
```

## Task 6: TUI App State, Rendering, And Input

**Files:**
- Create: `crates/aikit-tui/src/app.rs`
- Create: `crates/aikit-tui/src/ui.rs`
- Create: `crates/aikit-tui/src/input.rs`
- Modify: `crates/aikit-tui/src/main.rs`
- Test: `crates/aikit-tui/tests/app_state_tests.rs`

**Interfaces:**
- Produces: `AppState`.
- Produces: `FocusedPane`.
- Produces: `AppState::select_next()`, `AppState::focus_next_pane()`, `AppState::set_status(message: impl Into<String>)`.
- Produces: `handle_key(state: &mut AppState, key: KeyEvent) -> AppAction`.

- [ ] **Step 1: Write failing TUI state tests**

`crates/aikit-tui/tests/app_state_tests.rs`:

```rust
use aikit_tui::{app::{AppState, FocusedPane}, input::{AppAction, handle_key}};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[test]
fn tab_moves_focus_between_three_panes() {
    let mut state = AppState::default();
    assert_eq!(state.focused_pane, FocusedPane::Providers);

    let action = handle_key(&mut state, KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));

    assert_eq!(action, AppAction::None);
    assert_eq!(state.focused_pane, FocusedPane::Details);
}

#[test]
fn ctrl_s_requests_apply() {
    let mut state = AppState::default();
    let action = handle_key(&mut state, KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));

    assert_eq!(action, AppAction::ApplySelection);
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test -p aikit-tui --test app_state_tests`

Expected: FAIL because TUI app modules are not exported.

- [ ] **Step 3: Add TUI library exports**

Create `crates/aikit-tui/src/lib.rs`:

```rust
pub mod app;
pub mod input;
pub mod ui;
```

Update `crates/aikit-tui/Cargo.toml` with:

```toml
[lib]
name = "aikit_tui"
path = "src/lib.rs"
```

- [ ] **Step 4: Implement app state and input**

`crates/aikit-tui/src/app.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusedPane {
    Providers,
    Details,
    Targets,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppState {
    pub focused_pane: FocusedPane,
    pub status: String,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            focused_pane: FocusedPane::Providers,
            status: "Ready".into(),
        }
    }
}

impl AppState {
    pub fn focus_next_pane(&mut self) {
        self.focused_pane = match self.focused_pane {
            FocusedPane::Providers => FocusedPane::Details,
            FocusedPane::Details => FocusedPane::Targets,
            FocusedPane::Targets => FocusedPane::Providers,
        };
    }

    pub fn set_status(&mut self, message: impl Into<String>) {
        self.status = message.into();
    }
}
```

`crates/aikit-tui/src/input.rs`:

```rust
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::AppState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppAction {
    None,
    Quit,
    RefreshModels,
    ApplySelection,
}

pub fn handle_key(state: &mut AppState, key: KeyEvent) -> AppAction {
    match (key.code, key.modifiers) {
        (KeyCode::Char('q'), _) | (KeyCode::Esc, _) => AppAction::Quit,
        (KeyCode::Tab, _) => {
            state.focus_next_pane();
            AppAction::None
        }
        (KeyCode::Char('r'), _) => AppAction::RefreshModels,
        (KeyCode::Char('s'), KeyModifiers::CONTROL) => AppAction::ApplySelection,
        _ => AppAction::None,
    }
}
```

- [ ] **Step 5: Implement minimal Ratatui render and main loop**

`crates/aikit-tui/src/ui.rs` renders three titled blocks using `Layout`, `Block`, `Borders`, and `Paragraph`. `main.rs` initializes Ratatui terminal with Crossterm, handles key events through `handle_key`, redraws each tick, and restores the terminal on exit.

- [ ] **Step 6: Run TUI tests and workspace checks**

Run: `cargo test -p aikit-tui --test app_state_tests`

Expected: PASS for focus and apply action tests.

Run: `cargo test --workspace`

Expected: PASS for all workspace tests.

- [ ] **Step 7: Commit**

```bash
git add crates/aikit-tui
git commit -m "Add three-pane TUI shell"
```

## Task 7: CI, Release Builds, And Install Scripts

**Files:**
- Create: `.github/workflows/check.yml`
- Create: `.github/workflows/release.yml`
- Create: `scripts/install.sh`
- Create: `scripts/install.ps1`

**Interfaces:**
- Produces: CI for fmt, clippy, and tests.
- Produces: tag-triggered release artifacts for Linux, macOS, and Windows.
- Produces: install scripts that accept `AIKIT_REPO=owner/repo` and default to `aikit-rs/aikit`.

- [ ] **Step 1: Create check workflow**

`.github/workflows/check.yml`:

```yaml
name: check

on:
  pull_request:
  push:
    branches: [main, master]

jobs:
  rust:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt --check
      - run: cargo clippy --workspace --all-targets -- -D warnings
      - run: cargo test --workspace
```

- [ ] **Step 2: Create release workflow**

`.github/workflows/release.yml` builds `aikit` on `ubuntu-latest`, `macos-latest`, and `windows-latest`, uploads archives named `aikit-${{ matrix.target }}.*`, and attaches them to a GitHub Release when tags matching `v*` are pushed.

- [ ] **Step 3: Create install scripts**

`scripts/install.sh` behavior:

```sh
#!/usr/bin/env sh
set -eu
REPO="${AIKIT_REPO:-aikit-rs/aikit}"
VERSION="${AIKIT_VERSION:-latest}"
BIN_DIR="${AIKIT_BIN_DIR:-$HOME/.local/bin}"
```

It detects `uname -s` and `uname -m`, downloads the matching artifact from GitHub Releases, extracts `aikit`, marks it executable, and prints the installed path.

`scripts/install.ps1` behavior:

```powershell
$Repo = if ($env:AIKIT_REPO) { $env:AIKIT_REPO } else { "aikit-rs/aikit" }
$Version = if ($env:AIKIT_VERSION) { $env:AIKIT_VERSION } else { "latest" }
$BinDir = if ($env:AIKIT_BIN_DIR) { $env:AIKIT_BIN_DIR } else { Join-Path $HOME ".aikit\bin" }
```

It downloads the Windows artifact, extracts `aikit.exe`, and prints PATH guidance.

- [ ] **Step 4: Run workflow syntax and script checks**

Run: `cargo fmt --check`

Expected: PASS.

Run: `cargo clippy --workspace --all-targets -- -D warnings`

Expected: PASS.

Run: `cargo test --workspace`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add .github scripts
git commit -m "Add CI release and install scripts"
```

## Self-Review

- Spec coverage: The plan covers workspace structure, config TOML, OpenAI-compatible refresh, cache preservation, target backups, target writers, three-column TUI shell, CI, release artifacts, and install scripts.
- Placeholder scan: The plan contains no empty sections, unresolved implementation labels, or vague "fill this in" instructions.
- Type consistency: `AikitConfig`, `ProviderConfig`, `ModelCache`, `OpenAiCompatibleClient`, `TargetSelection`, `TargetWriter`, `TargetWriteResult`, `AppState`, `FocusedPane`, and `AppAction` are introduced before later tasks reference them.

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-06-27-aikit-implementation.md`. Two execution options:

1. **Subagent-Driven (recommended)** - I dispatch a fresh subagent per task, review between tasks, fast iteration.
2. **Inline Execution** - Execute tasks in this session using executing-plans, batch execution with checkpoints.

Which approach?
