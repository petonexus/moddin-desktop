//! REFramework module — STUB.
//!
//! Full design lives in [`docs/MODULES.md`](../docs/MODULES.md). The
//! install flow is gated on RE Engine detection and on the absence of
//! anti-cheat processes running.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReframeworkRequest {
    pub game_id: String,
    pub game_name: String,
    pub install_dir: String,
    pub executable: String,
    pub version_policy: String,
    #[serde(default)]
    pub release_api_url: Option<String>,
    #[serde(default)]
    pub pinned_tag: Option<String>,
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default)]
    pub mods: Vec<ReframeworkModRef>,
    #[serde(default)]
    pub safety_notes: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReframeworkModRef {
    pub id: String,
    pub script_url: String,
    #[serde(default)]
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReframeworkPreview {
    pub can_apply: bool,
    pub executable_exists: bool,
    pub engine_compatible: bool,
    pub anti_cheat_detected: bool,
    pub installed: bool,
    pub installed_version: Option<String>,
    pub changes: Vec<String>,
    pub warnings: Vec<String>,
}

#[tauri::command]
pub async fn preview_reframework(_request: ReframeworkRequest) -> Result<ReframeworkPreview, String> {
    Ok(ReframeworkPreview {
        can_apply: false,
        executable_exists: false,
        engine_compatible: false,
        anti_cheat_detected: false,
        installed: false,
        installed_version: None,
        changes: Vec::new(),
        warnings: vec![not_implemented()],
    })
}

#[tauri::command]
pub async fn install_reframework(_request: ReframeworkRequest) -> Result<serde_json::Value, String> {
    Err(not_implemented())
}

#[tauri::command]
pub async fn uninstall_reframework(_game_id: String) -> Result<serde_json::Value, String> {
    Err(not_implemented())
}

fn not_implemented() -> String {
    "The REFramework module is not implemented yet. See docs/MODULES.md for the design plan."
        .to_owned()
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