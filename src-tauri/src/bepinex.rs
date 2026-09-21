//! BepInEx module — STUB.
//!
//! Full design lives in [`docs/MODULES.md`](../docs/MODULES.md). The
//! install flow is gated on Unity / .NET engine detection and on the
//! IL2CPP vs Mono split, both of which require inspecting the game
//! executable before resolving the right archive.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BepinexRequest {
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
    pub unity_architecture: Option<String>,
    #[serde(default)]
    pub mods: Vec<BepinexModRef>,
    #[serde(default)]
    pub safety_notes: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BepinexModRef {
    pub id: String,
    pub archive_url: String,
    #[serde(default)]
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BepinexPreview {
    pub can_apply: bool,
    pub executable_exists: bool,
    pub engine_compatible: bool,
    pub architecture: Option<String>,
    pub installed: bool,
    pub installed_version: Option<String>,
    pub changes: Vec<String>,
    pub warnings: Vec<String>,
}

#[tauri::command]
pub async fn preview_bepinex(_request: BepinexRequest) -> Result<BepinexPreview, String> {
    Ok(BepinexPreview {
        can_apply: false,
        executable_exists: false,
        engine_compatible: false,
        architecture: None,
        installed: false,
        installed_version: None,
        changes: Vec::new(),
        warnings: vec![not_implemented()],
    })
}

#[tauri::command]
pub async fn install_bepinex(_request: BepinexRequest) -> Result<serde_json::Value, String> {
    Err(not_implemented())
}

#[tauri::command]
pub async fn uninstall_bepinex(_game_id: String) -> Result<serde_json::Value, String> {
    Err(not_implemented())
}

fn not_implemented() -> String {
    "The BepInEx module is not implemented yet. See docs/MODULES.md for the design plan."
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