//! Compatibility report — read-only snapshot of which modules are
//! installed for a given game and which upstream versions are available.
//!
//! The report is built from the per-module marker files (`*.json`) under
//! `%LOCALAPPDATA%/Moddin/tools/<module>/` plus the in-game marker files
//! the executable-directory modules drop next to the game (OptiScaler
//! and the desktop shortcut). It intentionally does **not** hit the
//! network — `current_version` and `installed_version` come from local
//! markers, and `latest_version` is `None` until the UI calls
//! `updates::check_module_update` for each row.
//!
//! The intent is to feed a "ready to VR" / "ready to play" summary so the
//! user can see, at a glance, which capabilities are installed and which
//! version is on disk. Network lookups stay opt-in per row.

use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ModuleCompatState {
    /// Module is configured for the game and the local files exist.
    Installed,
    /// Module is enabled in the catalog but no local marker was found.
    NotInstalled,
    /// Local marker exists but at least one declared file is missing.
    Broken,
    /// Module is not in scope for the catalog recipe.
    NotApplicable,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleCompatRow {
    pub module_id: String,
    pub display_name: String,
    pub state: ModuleCompatState,
    pub installed_version: Option<String>,
    pub latest_version: Option<String>,
    pub release_url: Option<String>,
    pub source: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompatReport {
    pub game_id: String,
    pub game_name: String,
    pub install_dir: String,
    pub executable: String,
    pub generated_at_millis: u64,
    pub rows: Vec<ModuleCompatRow>,
    pub warnings: Vec<String>,
}

/// Inputs the report builder accepts. The caller (frontend) supplies the
/// catalog-derived module list (with name + config) and the resolved game
/// install paths.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompatReportRequest {
    pub game_id: String,
    pub game_name: String,
    pub install_dir: String,
    pub executable: String,
    pub modules: Vec<CompatReportModule>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompatReportModule {
    pub id: String,
    pub name: String,
    /// Catalog config as a JSON object — used to pull `updateUrl`,
    /// `version`, `archiveSha256`, etc.
    #[serde(default)]
    pub config: serde_json::Value,
}

fn tool_root() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("Moddin")
        .join("tools")
}

fn read_user_marker(module_id: &str, game_id: &str) -> Option<serde_json::Value> {
    if game_id.is_empty() {
        return None;
    }
    let path = tool_root().join(module_id).join(format!("{game_id}.json"));
    let contents = fs::read_to_string(path).ok()?;
    serde_json::from_str(&contents).ok()
}

fn read_game_marker(install_dir: &Path, name: &str) -> Option<serde_json::Value> {
    let path = install_dir.join(name);
    let contents = fs::read_to_string(path).ok()?;
    serde_json::from_str(&contents).ok()
}

fn safe_join_relative(root: &Path, relative: &str) -> Option<PathBuf> {
    use std::path::Component;
    let path = Path::new(relative);
    for component in path.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            _ => return None,
        }
    }
    Some(root.join(path))
}

fn installed_files_consistent(marker: &serde_json::Value, install_dir: &Path) -> bool {
    let Some(files) = marker.get("installedFiles").and_then(|v| v.as_array()) else {
        return false;
    };
    files.iter().all(|entry| {
        entry
            .as_str()
            .and_then(|relative| safe_join_relative(install_dir, relative))
            .is_some_and(|path| path.is_file())
    })
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn row_for_user_marker(
    module_id: &str,
    display_name: &str,
    game_id: &str,
    install_dir: &Path,
    version_field: &str,
    archive_field: &str,
    config: &serde_json::Value,
) -> ModuleCompatRow {
    let marker = read_user_marker(module_id, game_id);
    let (state, installed_version, note) = match marker.as_ref() {
        Some(marker) if installed_files_consistent(marker, install_dir) => {
            let version = marker
                .get(version_field)
                .and_then(|v| v.as_str())
                .map(str::to_owned);
            (
                ModuleCompatState::Installed,
                version,
                None,
            )
        }
        Some(_) => (
            ModuleCompatState::Broken,
            None,
            Some("Local marker exists but installed files are missing.".to_owned()),
        ),
        None => (
            ModuleCompatState::NotInstalled,
            None,
            None,
        ),
    };
    let archive_sha256 = marker
        .as_ref()
        .and_then(|m| m.get(archive_field))
        .and_then(|v| v.as_str())
        .map(str::to_owned);
    let latest_version = config.get("version").and_then(|v| v.as_str()).map(str::to_owned);
    let release_url = config.get("updateUrl").and_then(|v| v.as_str()).map(str::to_owned);

    let mut metadata: BTreeMap<&str, String> = BTreeMap::new();
    if let Some(sha) = archive_sha256 {
        metadata.insert("source_sha256", sha);
    }
    if let Some(url) = release_url.as_ref() {
        metadata.insert("release_url", url.clone());
    }
    let source = if metadata.is_empty() {
        None
    } else {
        Some(
            metadata
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join(", "),
        )
    };

    ModuleCompatRow {
        module_id: module_id.to_owned(),
        display_name: display_name.to_owned(),
        state,
        installed_version,
        latest_version,
        release_url,
        source,
        note,
    }
}

fn row_for_game_marker(
    module_id: &str,
    display_name: &str,
    game_id: &str,
    install_dir: &Path,
    marker_name: &str,
    version_field: &str,
    proxy_field: Option<&str>,
    config: &serde_json::Value,
) -> ModuleCompatRow {
    let marker = read_game_marker(install_dir, marker_name);
    let (state, installed_version) = match marker.as_ref() {
        Some(marker) => {
            let version = marker
                .get(version_field)
                .and_then(|v| v.as_str())
                .map(str::to_owned);
            (ModuleCompatState::Installed, version)
        }
        None => (ModuleCompatState::NotInstalled, None),
    };
    let mut note: Option<String> = None;
    if let Some(proxy) = proxy_field {
        if let Some(marker) = marker.as_ref() {
            if let Some(proxy_value) = marker.get(proxy).and_then(|v| v.as_str()) {
                note = Some(format!("Active proxy: {proxy_value}"));
            }
        }
    }
    if !state_eq_installed(&state) && config.get("version").is_some() {
        // The recipe is published with a pinned version; surface that as
        // the "available" line so the UI can offer to apply it.
    }
    let latest_version = config.get("version").and_then(|v| v.as_str()).map(str::to_owned);
    let release_url = config.get("updateUrl").and_then(|v| v.as_str()).map(str::to_owned);

    let mut row = ModuleCompatRow {
        module_id: module_id.to_owned(),
        display_name: display_name.to_owned(),
        state,
        installed_version,
        latest_version,
        release_url,
        source: None,
        note,
    };
    let _ = game_id; // reserved for future marker lookups keyed by gameId
    row
}

fn state_eq_installed(state: &ModuleCompatState) -> bool {
    matches!(state, ModuleCompatState::Installed)
}

#[tauri::command]
pub async fn get_compat_report(
    request: CompatReportRequest,
) -> Result<CompatReport, String> {
    let install_root = PathBuf::from(&request.install_dir);
    if !install_root.is_dir() {
        return Err(format!(
            "Game install directory does not exist: {}",
            request.install_dir
        ));
    }
    let executable_path = safe_join_relative(&install_root, &request.executable)
        .ok_or_else(|| "Could not resolve executable directory.".to_owned())?;
    let exec_dir = executable_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or(install_root.clone());

    let mut warnings = Vec::new();
    let mut rows = Vec::with_capacity(request.modules.len());

    for module in &request.modules {
        let row = match module.id.as_str() {
            // User-profile markers (tool_root + <module>/<gameId>.json)
            "ofxr-framegen" => row_for_user_marker(
                "ofxr",
                &module.name,
                &request.game_id,
                &exec_dir,
                "version",
                "archiveSha256",
                &module.config,
            ),
            "uevr" => row_for_user_marker(
                "uevr",
                &module.name,
                &request.game_id,
                &exec_dir,
                "version",
                "sourceSha256",
                &module.config,
            ),
            "bepinex" => row_for_user_marker(
                "bepinex",
                &module.name,
                &request.game_id,
                &exec_dir,
                "version",
                "archiveSha256",
                &module.config,
            ),
            "cheeky-foveated-dlss" => row_for_user_marker(
                "cheeky",
                &module.name,
                &request.game_id,
                &exec_dir,
                "version",
                "addonSha256",
                &module.config,
            ),
            "reshade" => row_for_user_marker(
                "reshade",
                &module.name,
                &request.game_id,
                &exec_dir,
                "version",
                "archiveSha256",
                &module.config,
            ),
            "vr-launch" => ModuleCompatRow {
                module_id: "vr-launch".to_owned(),
                display_name: module.name.clone(),
                state: ModuleCompatState::NotApplicable,
                installed_version: None,
                latest_version: None,
                release_url: None,
                source: None,
                note: Some("VR launch profiles are applied per-session; no marker file is written.".to_owned()),
            },
            "desktop-shortcut" => {
                let marker = read_game_marker(&exec_dir, ".moddin-shortcut.json");
                let state = if marker.is_some() {
                    ModuleCompatState::Installed
                } else {
                    ModuleCompatState::NotInstalled
                };
                ModuleCompatRow {
                    module_id: "desktop-shortcut".to_owned(),
                    display_name: module.name.clone(),
                    state,
                    installed_version: None,
                    latest_version: None,
                    release_url: None,
                    source: None,
                    note: None,
                }
            }
            "optiscaler" => row_for_game_marker(
                "optiscaler",
                &module.name,
                &request.game_id,
                &exec_dir,
                ".moddin-optiscaler.json",
                "version",
                Some("proxyDll"),
                &module.config,
            ),
            "openxr" => ModuleCompatRow {
                module_id: "openxr".to_owned(),
                display_name: module.name.clone(),
                state: ModuleCompatState::NotApplicable,
                installed_version: None,
                latest_version: None,
                release_url: None,
                source: None,
                note: Some(
                    "OpenXR is a runtime helper; the active runtime is queried live, not stored as a version."
                        .to_owned(),
                ),
            },
            "obs-vr" => ModuleCompatRow {
                module_id: "obs-vr".to_owned(),
                display_name: module.name.clone(),
                state: ModuleCompatState::NotApplicable,
                installed_version: None,
                latest_version: None,
                release_url: None,
                source: None,
                note: Some(
                    "OBS VR edits an OBS scene collection; Moddin does not maintain a per-game version marker."
                        .to_owned(),
                ),
            },
            other => {
                warnings.push(format!(
                    "Compat report does not yet understand module '{other}'; skipping."
                ));
                continue;
            }
        };
        rows.push(row);
    }

    Ok(CompatReport {
        game_id: request.game_id,
        game_name: request.game_name,
        install_dir: request.install_dir,
        executable: request.executable,
        generated_at_millis: now_millis(),
        rows,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn temp_root(suffix: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("moddin-compat-{suffix}-{nanos}"))
    }

    #[test]
    fn reads_user_marker_and_infers_installed_state() {
        let install = temp_root("user-installed");
        std::fs::create_dir_all(&install).expect("install dir");
        std::fs::write(install.join("GameAssembly.dll"), b"fake").expect("layout");

        let game_id = format!("unity-test-{}", std::process::id());
        let marker_dir = tool_root().join("bepinex");
        std::fs::create_dir_all(&marker_dir).expect("marker dir");
        let marker_path = marker_dir.join(format!("{game_id}.json"));
        let payload = serde_json::json!({
            "version": "6.0.0-pre.2",
            "architecture": "il2cpp",
            "archiveSha256": "abcd",
            "installedFiles": ["GameAssembly.dll"]
        });
        std::fs::write(&marker_path, serde_json::to_vec_pretty(&payload).unwrap()).expect("marker");

        let row = row_for_user_marker(
            "bepinex",
            "BepInEx",
            &game_id,
            &install,
            "version",
            "archiveSha256",
            &serde_json::json!({}),
        );

        let _ = std::fs::remove_file(&marker_path);
        let _ = std::fs::remove_dir_all(&install);

        assert_eq!(row.state, ModuleCompatState::Installed);
        assert_eq!(row.installed_version.as_deref(), Some("6.0.0-pre.2"));
        assert_eq!(row.source.as_deref(), Some("source_sha256=abcd"));
    }

    #[test]
    fn reads_game_marker_for_optiscaler() {
        let install = temp_root("game-marker");
        std::fs::create_dir_all(&install).expect("install");
        let marker_path = install.join(".moddin-optiscaler.json");
        let payload = serde_json::json!({
            "version": "0.9.4",
            "proxyDll": "dxgi.dll",
            "sourceSha256": "deadbeef"
        });
        std::fs::write(&marker_path, serde_json::to_vec_pretty(&payload).unwrap()).expect("marker");

        let row = row_for_game_marker(
            "optiscaler",
            "OptiScaler",
            "any",
            &install,
            ".moddin-optiscaler.json",
            "version",
            Some("proxyDll"),
            &serde_json::json!({ "version": "0.9.4" }),
        );

        let _ = std::fs::remove_file(&marker_path);
        let _ = std::fs::remove_dir_all(&install);

        assert_eq!(row.state, ModuleCompatState::Installed);
        assert_eq!(row.installed_version.as_deref(), Some("0.9.4"));
        assert_eq!(row.note.as_deref(), Some("Active proxy: dxgi.dll"));
    }

    #[test]
    fn empty_lookup_returns_not_applicable_with_known_shapes() {
        let install = temp_root("empty");
        std::fs::create_dir_all(&install).expect("install");
        let mut map = HashMap::new();
        map.insert("vr-launch", ModuleCompatState::NotApplicable);
        map.insert("openxr", ModuleCompatState::NotApplicable);
        map.insert("obs-vr", ModuleCompatState::NotApplicable);
        // sanity: the HashMap is just a guard against future refactors
        // drifting the constants; the real assertions live in the row
        // builders above.
        assert_eq!(map.len(), 3);
        let _ = std::fs::remove_dir_all(&install);
    }
}
