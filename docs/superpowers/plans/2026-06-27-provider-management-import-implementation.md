# Provider Management Import Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add TUI-based provider/API key management and confirmed import from environment variables plus supported AI tool config files.

**Architecture:** Keep import and config mutation in `aikit-core`, with `aikit-tui` owning modal state, input routing, and user decisions. Provider edits, deletes, imports, and backups must be testable without terminal IO.

**Tech Stack:** Rust stable, Ratatui, Crossterm, Serde, TOML, Serde JSON, Directories, Tempfile, Wiremock, PowerShell/Bash installer scripts already present.

## Global Constraints

- `aikit-core` owns configuration, target writers, provider refresh, and import logic.
- `aikit-tui` owns the three-pane TUI, modal forms, confirmation dialogs, and user decisions.
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
- Config file scanners fail soft: missing or invalid files return warnings and continue.
- Startup import must not repeatedly nag after the user skips; store a source fingerprint and prompt only when the candidate fingerprint differs.
- Before destructive provider/API key changes, back up `aikit/config.toml` if it exists.

---

## File Structure

- Create `crates/aikit-core/src/import.rs`: import source enum, candidate types, environment scanner, target config scanners, merge logic, and fingerprinting.
- Create `crates/aikit-core/src/config_ops.rs`: provider/key add, edit, delete helpers, backup helpers, and validation types.
- Modify `crates/aikit-core/src/config.rs`: add import prompt state to `AikitConfig`.
- Modify `crates/aikit-core/src/lib.rs`: export `import` and `config_ops`.
- Create `crates/aikit-core/tests/import_tests.rs`: environment/config scanner and merge tests.
- Create `crates/aikit-core/tests/config_ops_tests.rs`: provider/key add/edit/delete and backup tests.
- Modify `crates/aikit-tui/src/app.rs`: modal state, provider/key form state, import prompt state, and commands that call core helpers.
- Modify `crates/aikit-tui/src/input.rs`: route `a/e/d/i/k/x`, modal field navigation, save/cancel, confirmation.
- Modify `crates/aikit-tui/src/ui.rs`: render provider/key modals, import prompt/list, confirmation dialogs, and masked secrets.
- Modify `crates/aikit-tui/tests/app_state_tests.rs`: modal and import flow tests.
- Modify `README.md`: document TUI provider management and import flow after implementation.

## Task 1: Core Import Model And Environment Scanner

**Files:**
- Create: `crates/aikit-core/src/import.rs`
- Modify: `crates/aikit-core/src/lib.rs`
- Test: `crates/aikit-core/tests/import_tests.rs`

**Interfaces:**
- Produces: `ImportSource`.
- Produces: `ImportCandidate`.
- Produces: `ImportPlan`.
- Produces: `scan_env(vars: impl IntoIterator<Item = (String, String)>) -> ImportPlan`.
- Produces: `candidate_fingerprint(candidates: &[ImportCandidate]) -> String`.

- [ ] **Step 1: Write failing environment scanner tests**

Create `crates/aikit-core/tests/import_tests.rs`:

```rust
use aikit_core::import::{candidate_fingerprint, scan_env, ImportSource};

#[test]
fn env_scan_imports_openai_key_base_url_and_model() {
    let plan = scan_env([
        ("OPENAI_API_KEY".to_string(), "sk-openai".to_string()),
        ("OPENAI_BASE_URL".to_string(), "https://api.openai.com/v1".to_string()),
        ("OPENAI_MODEL".to_string(), "gpt-4.1-mini".to_string()),
    ]);

    assert!(plan.warnings.is_empty());
    assert_eq!(plan.candidates.len(), 1);
    let candidate = &plan.candidates[0];
    assert_eq!(candidate.source, ImportSource::Env);
    assert_eq!(candidate.provider_id, "openai");
    assert_eq!(candidate.provider_name, "OpenAI");
    assert_eq!(candidate.base_url.as_deref(), Some("https://api.openai.com/v1"));
    assert_eq!(candidate.api_key_name.as_deref(), Some("OPENAI_API_KEY"));
    assert_eq!(candidate.api_key_value.as_deref(), Some("sk-openai"));
    assert_eq!(candidate.model.as_deref(), Some("gpt-4.1-mini"));
}

#[test]
fn env_scan_imports_anthropic_model_variable() {
    let plan = scan_env([
        ("ANTHROPIC_API_KEY".to_string(), "sk-ant".to_string()),
        ("ANTHROPIC_BASE_URL".to_string(), "https://anthropic-proxy.example/v1".to_string()),
        ("ANTHROPIC_MODEL".to_string(), "claude-sonnet-4".to_string()),
    ]);

    let candidate = &plan.candidates[0];
    assert_eq!(candidate.provider_id, "anthropic");
    assert_eq!(candidate.model.as_deref(), Some("claude-sonnet-4"));
}

#[test]
fn env_scan_candidate_fingerprint_changes_when_secret_changes() {
    let first = scan_env([("OPENAI_API_KEY".to_string(), "sk-one".to_string())]);
    let second = scan_env([("OPENAI_API_KEY".to_string(), "sk-two".to_string())]);

    assert_ne!(
        candidate_fingerprint(&first.candidates),
        candidate_fingerprint(&second.candidates)
    );
}
```

- [ ] **Step 2: Run test to verify failure**

Run: `cargo test -p aikit-core --test import_tests`

Expected: FAIL because `aikit_core::import` does not exist.

- [ ] **Step 3: Implement import model and environment scanner**

Add `pub mod import;` to `crates/aikit-core/src/lib.rs`.

Create `crates/aikit-core/src/import.rs` with:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ImportSource {
    Env,
    Claude,
    Gemini,
    Codex,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportCandidate {
    pub source: ImportSource,
    pub provider_id: String,
    pub provider_name: String,
    pub base_url: Option<String>,
    pub api_key_name: Option<String>,
    pub api_key_value: Option<String>,
    pub model: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportPlan {
    pub candidates: Vec<ImportCandidate>,
    pub warnings: Vec<String>,
}
```

Implement `scan_env` by grouping these variables:

```rust
OPENAI_API_KEY, OPENAI_BASE_URL, OPENAI_MODEL
ANTHROPIC_API_KEY, ANTHROPIC_BASE_URL, ANTHROPIC_MODEL
GEMINI_API_KEY, GEMINI_BASE_URL, GEMINI_MODEL
```

Defaults:

```rust
openai -> OpenAI
anthropic -> Anthropic
gemini -> Gemini
```

Only create a candidate when at least one of key/base URL/model exists.

Implement `candidate_fingerprint` by sorting candidates by `(source, provider_id, base_url, api_key_name, api_key_value, model)`, joining fields with separators, and hashing with `std::collections::hash_map::DefaultHasher`. Return lowercase hex.

- [ ] **Step 4: Run environment scanner tests**

Run: `cargo test -p aikit-core --test import_tests`

Expected: PASS for the three environment scanner tests.

- [ ] **Step 5: Commit**

```bash
git add crates/aikit-core/src/lib.rs crates/aikit-core/src/import.rs crates/aikit-core/tests/import_tests.rs
git commit -m "feat(core): add import candidate scanner"
```

## Task 2: Config File Scanners And Import Merge

**Files:**
- Modify: `crates/aikit-core/src/import.rs`
- Modify: `crates/aikit-core/src/config.rs`
- Test: `crates/aikit-core/tests/import_tests.rs`

**Interfaces:**
- Consumes: `ImportCandidate`, `ImportPlan`, `candidate_fingerprint`.
- Produces: `scan_claude_config(path: &Path) -> ImportPlan`.
- Produces: `scan_gemini_config(path: &Path) -> ImportPlan`.
- Produces: `scan_codex_config(path: &Path) -> ImportPlan`.
- Produces: `apply_import_candidates(config: &mut AikitConfig, selected: &[ImportCandidate]) -> ImportResult`.
- Produces: `ImportPromptState { skipped_fingerprint: Option<String> }` stored on `AikitConfig`.

- [ ] **Step 1: Extend import tests for config scanners and merge**

Append to `crates/aikit-core/tests/import_tests.rs`:

```rust
use aikit_core::{
    config::{AikitConfig, ApiKeyConfig, ModelCache, ProviderConfig},
    import::{
        apply_import_candidates, scan_codex_config, scan_gemini_config, ImportCandidate,
        ImportSource,
    },
};
use tempfile::tempdir;

#[test]
fn codex_scan_reads_aikit_provider_fields() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(
        &path,
        r#"
model = "model-from-codex"
model_provider = "aikit"

[model_providers.aikit]
name = "aikit"
base_url = "https://proxy.example/v1"
api_key = "sk-codex"
"#,
    )
    .unwrap();

    let plan = scan_codex_config(&path);

    assert!(plan.warnings.is_empty());
    let candidate = &plan.candidates[0];
    assert_eq!(candidate.source, ImportSource::Codex);
    assert_eq!(candidate.base_url.as_deref(), Some("https://proxy.example/v1"));
    assert_eq!(candidate.api_key_value.as_deref(), Some("sk-codex"));
    assert_eq!(candidate.model.as_deref(), Some("model-from-codex"));
}

#[test]
fn invalid_gemini_config_returns_warning_not_error() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");
    std::fs::write(&path, "{invalid").unwrap();

    let plan = scan_gemini_config(&path);

    assert!(plan.candidates.is_empty());
    assert_eq!(plan.warnings.len(), 1);
}

#[test]
fn merge_preserves_existing_name_enabled_and_model_cache() {
    let mut config = AikitConfig {
        providers: vec![ProviderConfig {
            id: "existing".into(),
            name: "Custom Name".into(),
            base_url: "https://proxy.example/v1".into(),
            enabled: false,
            api_keys: vec![ApiKeyConfig {
                id: "default".into(),
                name: "Default".into(),
                value: "sk-existing".into(),
            }],
            models_cache: Some(ModelCache {
                refreshed_at: "old".into(),
                models: vec!["cached-model".into()],
                last_error: None,
            }),
        }],
        ..AikitConfig::default()
    };

    let result = apply_import_candidates(
        &mut config,
        &[ImportCandidate {
            source: ImportSource::Env,
            provider_id: "openai".into(),
            provider_name: "OpenAI".into(),
            base_url: Some("https://proxy.example/v1".into()),
            api_key_name: Some("Imported".into()),
            api_key_value: Some("sk-imported".into()),
            model: Some("imported-model".into()),
            warnings: vec![],
        }],
    );

    assert_eq!(result.updated_providers, 1);
    assert_eq!(config.providers.len(), 1);
    assert_eq!(config.providers[0].name, "Custom Name");
    assert!(!config.providers[0].enabled);
    assert_eq!(
        config.providers[0].models_cache.as_ref().unwrap().models,
        vec!["cached-model"]
    );
    assert_eq!(config.providers[0].api_keys.len(), 2);
}
```

- [ ] **Step 2: Run test to verify failure**

Run: `cargo test -p aikit-core --test import_tests`

Expected: FAIL because config scanners, merge function, and prompt state do not exist.

- [ ] **Step 3: Implement prompt state and scanners**

Modify `AikitConfig` in `crates/aikit-core/src/config.rs`:

```rust
pub import_prompt: ImportPromptState,
```

Add:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ImportPromptState {
    pub skipped_fingerprint: Option<String>,
}
```

Update every `AikitConfig` construction in tests to include `..AikitConfig::default()` where possible or set `import_prompt`.

Implement scanner functions in `import.rs`:

- `scan_codex_config`: parse TOML root table, read root `model`, `model_provider`, and `[model_providers.<model_provider>]` table. Use `base_url` and `api_key` from selected provider table.
- `scan_claude_config` and `scan_gemini_config`: parse JSON object, read `aikit.base_url`, `aikit.api_key`, and `aikit.model` if present.
- Missing file returns empty plan with no warnings.
- Invalid file returns empty candidates plus one warning.

- [ ] **Step 4: Implement conservative merge**

Implement:

```rust
pub struct ImportResult {
    pub added_providers: usize,
    pub updated_providers: usize,
    pub added_keys: usize,
    pub active_selection_updated: bool,
    pub warnings: Vec<String>,
}

pub fn apply_import_candidates(
    config: &mut AikitConfig,
    selected: &[ImportCandidate],
) -> ImportResult
```

Rules:

- Find provider by matching candidate `base_url` with existing `provider.base_url`.
- If no base URL match, match by `provider.id == candidate.provider_id`.
- Existing `name`, `enabled`, and `models_cache` are not overwritten.
- Add missing provider if no match exists.
- Add key when `api_key_value` exists and no existing key has same ID or same value.
- Key ID uses candidate `api_key_name` normalized to lowercase alphanumeric plus dashes; fallback `imported`.
- If candidate has model and no active selection exists, set `active_selection` to provider/key/model when a key exists.
- Do not create fake `models_cache`.

- [ ] **Step 5: Run import tests**

Run: `cargo test -p aikit-core --test import_tests`

Expected: PASS for environment, scanner, and merge tests.

- [ ] **Step 6: Commit**

```bash
git add crates/aikit-core/src/config.rs crates/aikit-core/src/import.rs crates/aikit-core/tests/import_tests.rs
git commit -m "feat(core): merge imported provider candidates"
```

## Task 3: Provider And API Key Config Operations

**Files:**
- Create: `crates/aikit-core/src/config_ops.rs`
- Modify: `crates/aikit-core/src/lib.rs`
- Test: `crates/aikit-core/tests/config_ops_tests.rs`

**Interfaces:**
- Produces: `ProviderForm`.
- Produces: `ApiKeyForm`.
- Produces: `add_provider`, `update_provider`, `delete_provider`.
- Produces: `add_api_key`, `update_api_key`, `delete_api_key`.
- Produces: `backup_config_file(path: &Path) -> Result<Option<PathBuf>>`.

- [ ] **Step 1: Write failing config operation tests**

Create `crates/aikit-core/tests/config_ops_tests.rs`:

```rust
use aikit_core::{
    config::{ActiveSelection, AikitConfig, ApiKeyConfig, ModelCache, ProviderConfig},
    config_ops::{
        add_api_key, add_provider, backup_config_file, delete_api_key, delete_provider, ApiKeyForm,
        ProviderForm,
    },
};
use tempfile::tempdir;

#[test]
fn add_provider_validates_unique_id_and_url() {
    let mut config = AikitConfig::default();

    add_provider(
        &mut config,
        ProviderForm {
            id: "openrouter".into(),
            name: "OpenRouter".into(),
            base_url: "https://openrouter.ai/api/v1".into(),
            enabled: true,
        },
    )
    .unwrap();

    assert_eq!(config.providers.len(), 1);
    assert!(add_provider(
        &mut config,
        ProviderForm {
            id: "openrouter".into(),
            name: "Duplicate".into(),
            base_url: "https://dup.example/v1".into(),
            enabled: true,
        },
    )
    .is_err());
}

#[test]
fn delete_provider_clears_active_selection_and_cache() {
    let mut config = sample_config();

    delete_provider(&mut config, "provider").unwrap();

    assert!(config.providers.is_empty());
    assert!(config.active_selection.is_none());
}

#[test]
fn delete_api_key_clears_active_selection_for_that_key() {
    let mut config = sample_config();

    delete_api_key(&mut config, "provider", "key").unwrap();

    assert!(config.providers[0].api_keys.is_empty());
    assert!(config.active_selection.is_none());
}

#[test]
fn backup_config_file_copies_existing_file() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "version = 1").unwrap();

    let backup = backup_config_file(&path).unwrap().unwrap();

    assert!(backup.exists());
    assert_eq!(std::fs::read_to_string(backup).unwrap(), "version = 1");
}

fn sample_config() -> AikitConfig {
    AikitConfig {
        providers: vec![ProviderConfig {
            id: "provider".into(),
            name: "Provider".into(),
            base_url: "https://example.com/v1".into(),
            enabled: true,
            api_keys: vec![ApiKeyConfig {
                id: "key".into(),
                name: "Key".into(),
                value: "sk".into(),
            }],
            models_cache: Some(ModelCache {
                refreshed_at: "old".into(),
                models: vec!["model".into()],
                last_error: None,
            }),
        }],
        active_selection: Some(ActiveSelection {
            provider_id: "provider".into(),
            api_key_id: "key".into(),
            model_id: "model".into(),
        }),
        ..AikitConfig::default()
    }
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test -p aikit-core --test config_ops_tests`

Expected: FAIL because `config_ops` module does not exist.

- [ ] **Step 3: Implement config operations**

Create `crates/aikit-core/src/config_ops.rs`:

```rust
pub struct ProviderForm {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub enabled: bool,
}

pub struct ApiKeyForm {
    pub id: String,
    pub name: String,
    pub value: String,
}
```

Functions:

- `add_provider`: validate non-empty ID/name/base URL, parse base URL with `reqwest::Url::parse`, reject duplicate provider ID, push provider with empty keys/cache.
- `update_provider`: find by old ID, validate new form, reject duplicate new ID, preserve existing keys/cache, update active selection provider ID if old ID changes.
- `delete_provider`: remove provider; if active selection points to it, clear active selection.
- `add_api_key`: validate non-empty ID/name/value, reject duplicate key ID under provider.
- `update_api_key`: update key fields; if key ID changes and active selection points to old key, update active selection key ID.
- `delete_api_key`: remove key; if active selection points to it, clear active selection.
- `backup_config_file`: if path exists, copy to `config.toml.bak.<timestamp-ms>` next to original; if missing, return `Ok(None)`.

Add `pub mod config_ops;` to `lib.rs`.

- [ ] **Step 4: Run config operation tests**

Run: `cargo test -p aikit-core --test config_ops_tests`

Expected: PASS for add/delete/backup tests.

- [ ] **Step 5: Commit**

```bash
git add crates/aikit-core/src/lib.rs crates/aikit-core/src/config_ops.rs crates/aikit-core/tests/config_ops_tests.rs
git commit -m "feat(core): add provider config operations"
```

## Task 4: TUI Modal State And Provider Editing

**Files:**
- Modify: `crates/aikit-tui/src/app.rs`
- Modify: `crates/aikit-tui/src/input.rs`
- Modify: `crates/aikit-tui/src/ui.rs`
- Modify: `crates/aikit-tui/tests/app_state_tests.rs`

**Interfaces:**
- Consumes: `ProviderForm`, `ApiKeyForm`, config operation helpers.
- Produces: `ModalState`.
- Produces: app methods for opening provider/key modals, editing fields, saving, canceling, and confirming delete.

- [ ] **Step 1: Add failing TUI modal tests**

Append to `crates/aikit-tui/tests/app_state_tests.rs`:

```rust
#[test]
fn provider_modal_save_adds_provider() {
    let mut state = AppState::from_config(
        std::path::PathBuf::from("config.toml"),
        AikitConfig::default(),
    );

    state.open_add_provider_modal();
    state.set_modal_field("id", "openrouter").unwrap();
    state.set_modal_field("name", "OpenRouter").unwrap();
    state
        .set_modal_field("base_url", "https://openrouter.ai/api/v1")
        .unwrap();
    state.save_modal().unwrap();

    assert_eq!(state.config.providers[0].id, "openrouter");
}

#[test]
fn api_key_modal_save_adds_key_to_selected_provider() {
    let mut state = AppState::from_config(
        std::path::PathBuf::from("config.toml"),
        AikitConfig {
            providers: vec![ProviderConfig {
                id: "provider".into(),
                name: "Provider".into(),
                base_url: "https://example.com/v1".into(),
                enabled: true,
                api_keys: vec![],
                models_cache: None,
            }],
            ..AikitConfig::default()
        },
    );

    state.open_add_api_key_modal().unwrap();
    state.set_modal_field("id", "default").unwrap();
    state.set_modal_field("name", "Default").unwrap();
    state.set_modal_field("value", "sk-test").unwrap();
    state.save_modal().unwrap();

    assert_eq!(state.config.providers[0].api_keys[0].id, "default");
}

#[test]
fn delete_provider_confirmation_clears_provider() {
    let mut state = AppState::from_config(
        std::path::PathBuf::from("config.toml"),
        sample_config(std::path::PathBuf::from("codex.toml")),
    );

    state.open_delete_provider_confirmation().unwrap();
    state.confirm_modal().unwrap();

    assert!(state.config.providers.is_empty());
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test -p aikit-tui --test app_state_tests`

Expected: FAIL because modal APIs do not exist.

- [ ] **Step 3: Implement app modal state**

Add to `app.rs`:

```rust
pub enum ModalState {
    None,
    ProviderForm(ProviderFormState),
    ApiKeyForm(ApiKeyFormState),
    ConfirmDeleteProvider { provider_id: String },
    ConfirmDeleteApiKey { provider_id: String, api_key_id: String },
}
```

Form states contain ordered fields, current field index, values, and optional validation error.

Implement:

- `open_add_provider_modal`
- `open_edit_provider_modal`
- `open_add_api_key_modal`
- `open_edit_api_key_modal`
- `open_delete_provider_confirmation`
- `open_delete_api_key_confirmation`
- `set_modal_field`
- `modal_next_field`
- `modal_previous_field`
- `save_modal`
- `confirm_modal`
- `cancel_modal`

`save_modal` calls `aikit_core::config_ops` helpers, then saves config to `config_path` for real file-backed states. In tests with a relative dummy path, allow save to be skipped only when parent is absent and document this in the method.

- [ ] **Step 4: Wire input and rendering**

Update `input.rs`:

- If a modal is open:
  - `Esc` cancels.
  - `Tab` next field.
  - `BackTab` previous field.
  - `Enter` saves or confirms.
  - Printable chars append to current field.
  - `Backspace` deletes from current field.
- If no modal:
  - `a`, `e`, `d`, `k`, `x` open corresponding modals.

Update `ui.rs` to render a centered text modal over the existing panes:

- Provider form with masked values where relevant.
- API key form with masked value outside active edit field.
- Delete confirmation.
- Field-level error line.

- [ ] **Step 5: Run TUI modal tests**

Run: `cargo test -p aikit-tui --test app_state_tests`

Expected: PASS for existing and modal tests.

- [ ] **Step 6: Commit**

```bash
git add crates/aikit-tui/src/app.rs crates/aikit-tui/src/input.rs crates/aikit-tui/src/ui.rs crates/aikit-tui/tests/app_state_tests.rs
git commit -m "feat(tui): add provider management modals"
```

## Task 5: TUI Import Prompt And Import Application

**Files:**
- Modify: `crates/aikit-tui/src/app.rs`
- Modify: `crates/aikit-tui/src/input.rs`
- Modify: `crates/aikit-tui/src/ui.rs`
- Modify: `crates/aikit-tui/tests/app_state_tests.rs`

**Interfaces:**
- Consumes: `scan_env`, config scanners, `apply_import_candidates`.
- Produces: `AppState::scan_import_candidates`.
- Produces: import prompt modal, skip behavior, import confirmation.

- [ ] **Step 1: Add failing TUI import flow tests**

Append to `crates/aikit-tui/tests/app_state_tests.rs`:

```rust
#[test]
fn import_prompt_skip_stores_fingerprint_without_writing_provider() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("aikit").join("config.toml");
    AikitConfig::default().save_to(&config_path).unwrap();
    let mut state = AppState::new(config_path.clone());
    state.load_config().unwrap();

    state.set_import_candidates_for_test(vec![aikit_core::import::ImportCandidate {
        source: aikit_core::import::ImportSource::Env,
        provider_id: "openai".into(),
        provider_name: "OpenAI".into(),
        base_url: Some("https://api.openai.com/v1".into()),
        api_key_name: Some("OPENAI_API_KEY".into()),
        api_key_value: Some("sk-test".into()),
        model: Some("gpt-4.1-mini".into()),
        warnings: vec![],
    }]);
    state.open_import_prompt().unwrap();
    state.skip_import_prompt().unwrap();

    let saved = AikitConfig::load_from(&config_path).unwrap();
    assert!(saved.providers.is_empty());
    assert!(saved.import_prompt.skipped_fingerprint.is_some());
}

#[test]
fn import_prompt_confirm_writes_provider() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("aikit").join("config.toml");
    AikitConfig::default().save_to(&config_path).unwrap();
    let mut state = AppState::new(config_path.clone());
    state.load_config().unwrap();

    state.set_import_candidates_for_test(vec![aikit_core::import::ImportCandidate {
        source: aikit_core::import::ImportSource::Env,
        provider_id: "openai".into(),
        provider_name: "OpenAI".into(),
        base_url: Some("https://api.openai.com/v1".into()),
        api_key_name: Some("OPENAI_API_KEY".into()),
        api_key_value: Some("sk-test".into()),
        model: Some("gpt-4.1-mini".into()),
        warnings: vec![],
    }]);
    state.open_import_prompt().unwrap();
    state.confirm_import_all().unwrap();

    let saved = AikitConfig::load_from(&config_path).unwrap();
    assert_eq!(saved.providers.len(), 1);
    assert_eq!(saved.providers[0].api_keys[0].value, "sk-test");
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test -p aikit-tui --test app_state_tests`

Expected: FAIL because import prompt APIs do not exist.

- [ ] **Step 3: Implement import prompt state**

Add modal state variants:

- `ImportPrompt { candidates, fingerprint, selected_indices, warnings }`
- `ImportList { candidates, fingerprint, selected_indices, cursor }`

Add app methods:

- `scan_import_candidates(&mut self) -> ImportPlan`
- `open_import_prompt(&mut self) -> Result<()>`
- `confirm_import_all(&mut self) -> Result<()>`
- `skip_import_prompt(&mut self) -> Result<()>`
- `toggle_import_candidate(&mut self)`
- `confirm_selected_imports(&mut self) -> Result<()>`
- `set_import_candidates_for_test`

Startup behavior in `main.rs`:

- After `state.load_config()`, if `state.config.providers.is_empty()`, scan import sources.
- If candidates exist and fingerprint differs from `config.import_prompt.skipped_fingerprint`, open import prompt.

Manual behavior:

- `i` opens import prompt regardless of skipped fingerprint.

- [ ] **Step 4: Render import prompt and mask secrets**

Update `ui.rs`:

- Import prompt shows count, source names, masked key previews, base URLs, models, and warnings.
- Import list shows selectable candidates.
- Mask secrets as first four chars plus last four chars when long enough; otherwise show `***`.

Update `input.rs`:

- No modal: `i` opens import prompt action.
- Import prompt:
  - `Enter`: import all.
  - `Esc`: skip.
  - `Tab` or `l`: open list.
- Import list:
  - `Space`: toggle candidate.
  - `Up`/`Down`: move cursor.
  - `Enter`: import selected.
  - `Esc`: cancel.

- [ ] **Step 5: Run import prompt tests**

Run: `cargo test -p aikit-tui --test app_state_tests`

Expected: PASS for existing, modal, and import prompt tests.

- [ ] **Step 6: Commit**

```bash
git add crates/aikit-tui/src/app.rs crates/aikit-tui/src/input.rs crates/aikit-tui/src/main.rs crates/aikit-tui/src/ui.rs crates/aikit-tui/tests/app_state_tests.rs
git commit -m "feat(tui): add provider import prompt"
```

## Task 6: README Usage Update And Full Verification

**Files:**
- Modify: `README.md`

**Interfaces:**
- Consumes completed TUI provider management and import behavior.
- Produces user-facing documentation for provider management and import.

- [ ] **Step 1: Update README**

Replace the manual-only provider section with:

- TUI provider management keys: `a`, `e`, `d`, `k`, `x`.
- Import key: `i`.
- Startup import prompt behavior.
- Supported environment variables:
  - `OPENAI_API_KEY`, `OPENAI_BASE_URL`, `OPENAI_MODEL`
  - `ANTHROPIC_API_KEY`, `ANTHROPIC_BASE_URL`, `ANTHROPIC_MODEL`
  - `GEMINI_API_KEY`, `GEMINI_BASE_URL`, `GEMINI_MODEL`
- Security note: imported API keys are saved in local TOML as plain text.

- [ ] **Step 2: Run full checks**

Run:

```powershell
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Expected: all commands pass.

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs: update provider management usage"
```

## Self-Review

- Spec coverage: The plan covers TUI provider add/edit/delete, API key add/edit/delete, import from environment variables, import from Claude/Gemini/Codex config files, conservative merge rules, prompt skip fingerprint, backups, masking, and README updates.
- Placeholder scan: The plan contains no placeholder sections or unresolved values.
- Type consistency: `ImportCandidate`, `ImportPlan`, `ImportResult`, `ImportPromptState`, `ProviderForm`, `ApiKeyForm`, and modal methods are introduced before later tasks consume them.

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-06-27-provider-management-import-implementation.md`. Two execution options:

1. **Subagent-Driven (recommended)** - I dispatch a fresh subagent per task, review between tasks, fast iteration.
2. **Inline Execution** - Execute tasks in this session using executing-plans, batch execution with checkpoints.

Which approach?
