use super::{ApiError, AppState};
use aikit_core::updater::{self, LATEST_RELEASE_URL};
use axum::{extract::State, Json};

pub async fn check(
    State(state): State<AppState>,
) -> Result<Json<updater::UpdateCheckOutcome>, ApiError> {
    let outcome = updater::check_for_updates(&state.client, LATEST_RELEASE_URL).await?;
    Ok(Json(outcome))
}
