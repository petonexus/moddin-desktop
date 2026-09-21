//! ReShade module — STUB.
//!
//! This file establishes the surface the catalog, UI, and transaction
//! store expect from the future ReShade module. The concrete download,
//! install, verification, and rollback logic is described in
//! [`docs/MODULES.md`](../docs/MODULES.md) and will land in a follow-up
//! PR once the upstream feed is locked in.
//!
//! Until then, the commands below return explicit "not implemented"
//! errors so the UI can advertise ReShade as `planned` without breaking
//! builds or pretending the install works.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReshadeRequest {
    pub game_id: String,
    pub game_name: String,
    pub install_dir: String,
    pub executable: String,
    pub version_policy: String,
    pub release_api_url: Option<String>,
    pub pinned_tag: Option<String>,
    pub sha256: Option<String>,
    #[serde(default)]
    pub proxy_candidates: Vec<String>,
    #[serde(default)]
    pub safety_notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReshadePreview {
    pub can_apply: bool,
    pub executable_exists: bool,
    pub installed: bool,
    pub installed_version: Option<String>,
    pub proxy_chosen: Option<String>,
    pub changes: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReshadeStubError {
    pub message: &'static str,
    pub design_doc: &'static str,
}

const NOT_IMPLEMENTED: &str =
    "The ReShade module is not implemented yet. See docs/MODULES.md for the design plan.";

fn not_implemented() -> String {
    NOT_IMPLEMENTED.to_owned()
}

#[tauri::command]
pub async fn preview_reshade(_request: ReshadeRequest) -> Result<ReshadePreview, String> {
    Ok(ReshadePreview {
        can_apply: false,
        executable_exists: false,
        installed: false,
        installed_version: None,
        proxy_chosen: None,
        changes: Vec::new(),
        warnings: vec![not_implemented()],
    })
}

#[tauri::command]
pub async fn install_reshade(_request: ReshadeRequest) -> Result<serde_json::Value, String> {
    Err(not_implemented())
}

#[tauri::command]
pub async fn uninstall_reshade(_game_id: String) -> Result<serde_json::Value, String> {
    Err(not_implemented())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_explains_where_to_find_the_design() {
        let error = not_implemented();
        assert!(error.contains("docs/MODULES.md"));
    }
}