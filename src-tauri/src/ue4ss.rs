//! UE4SS module — STUB.
//!
//! Full design lives in [`docs/MODULES.md`](../docs/MODULES.md). This
//! file establishes the surface the catalog, UI, and transaction store
//! expect from the future UE4SS module. The concrete download, install,
//! and rollback logic lands in a follow-up PR.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ue4ssRequest {
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
    pub proxy_candidates: Vec<String>,
    #[serde(default)]
    pub mods: Vec<Ue4ssModRef>,
    #[serde(default)]
    pub safety_notes: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ue4ssModRef {
    pub id: String,
    pub archive_url: String,
    #[serde(default)]
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ue4ssPreview {
    pub can_apply: bool,
    pub executable_exists: bool,
    pub installed: bool,
    pub installed_version: Option<String>,
    pub engine_compatible: bool,
    pub changes: Vec<String>,
    pub warnings: Vec<String>,
}

#[tauri::command]
pub async fn preview_ue4ss(_request: Ue4ssRequest) -> Result<Ue4ssPreview, String> {
    Ok(Ue4ssPreview {
        can_apply: false,
        executable_exists: false,
        installed: false,
        installed_version: None,
        engine_compatible: false,
        changes: Vec::new(),
        warnings: vec![not_implemented()],
    })
}

#[tauri::command]
pub async fn install_ue4ss(_request: Ue4ssRequest) -> Result<serde_json::Value, String> {
    Err(not_implemented())
}

#[tauri::command]
pub async fn uninstall_ue4ss(_game_id: String) -> Result<serde_json::Value, String> {
    Err(not_implemented())
}

fn not_implemented() -> String {
    "The UE4SS module is not implemented yet. See docs/MODULES.md for the design plan."
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