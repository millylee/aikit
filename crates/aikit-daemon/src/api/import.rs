use super::{mutate_config, ApiError, AppState};
use aikit_core::import::{
    apply_import_candidates, scan_claude_config, scan_codex_config, scan_env, ImportCandidate,
    ImportPlan,
};
use aikit_core::targets::TargetWriter;
use aikit_core::targets::{claude::ClaudeWriter, codex::CodexWriter};
use axum::{extract::State, Json};

pub async fn scan(State(_state): State<AppState>) -> Result<Json<ImportPlan>, ApiError> {
    let mut plan = scan_env(std::env::vars());
    append_plan(
        &mut plan,
        scan_at_default_path(&ClaudeWriter, scan_claude_config),
    );
    append_plan(
        &mut plan,
        scan_at_default_path(&CodexWriter, scan_codex_config),
    );
    Ok(Json(plan))
}

fn scan_at_default_path(
    writer: &dyn TargetWriter,
    scan: fn(&std::path::Path) -> ImportPlan,
) -> ImportPlan {
    match writer.default_path() {
        Ok(path) => scan(&path),
        Err(_) => ImportPlan::default(),
    }
}

fn append_plan(base: &mut ImportPlan, plan: ImportPlan) {
    base.candidates.extend(plan.candidates);
    base.warnings.extend(plan.warnings);
}

#[derive(serde::Deserialize)]
pub struct ImportApplyPayload {
    pub candidates: Vec<ImportCandidate>,
}

pub async fn apply(
    State(state): State<AppState>,
    Json(payload): Json<ImportApplyPayload>,
) -> Result<Json<aikit_core::import::ImportResult>, ApiError> {
    let mut result = aikit_core::import::ImportResult::default();
    mutate_config(&state, |config| {
        result = apply_import_candidates(config, &payload.candidates);
        Ok(())
    })?;
    Ok(Json(result))
}
