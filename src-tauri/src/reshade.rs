//! ReShade host installer.
//!
//! This module installs a ReShade build compatible with the selected
//! game. The Cheeky Foveated DLSS add-on already ships with its own
//! ReShade add-on (.addon64) handling in [`crate::cheeky`]; this module
//! manages the **host** ReShade binary, INI, and proxy DLL placement.
//!
//! ## Scope (v1)
//!
//! - Resolve a ReShade archive (local path or pinned URL).
//! - Verify SHA-256 when the recipe declares one.
//! - Extract `ReShade64.dll` and `ReShadeAddons64.dll` (when present)
//!   next to the game executable, renaming the host DLL to the chosen
//!   proxy (`dxgi.dll`, `version.dll`, `dinput8.dll`, etc.).
//! - Drop a default `ReShade.ini` so the user can edit later.
//! - Record a transaction so the install is reversible.
//!
//! ## Out of scope (for now)
//!
//! - Live GitHub-release resolution. The recipe must provide
//!   `archiveUrl` (pinned) or `localArchive` (absolute path) for now.
//! - Automatic add-on selection beyond the bundled Cheeky path.
//! - INI generation per-game presets.

use crate::transaction::{self, TransactionRecord};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{Cursor, Read, Write},
    path::{Component, Path, PathBuf},
};
use zip::ZipArchive;

const MARKER_FILE: &str = ".moddin-reshade.json";
const OPTISCALER_MARKER_FILE: &str = ".moddin-optiscaler.json";
const HOST_DLL_NAME: &str = "ReShade64.dll";
const ADDON_DLL_NAME: &str = "ReShadeAddons64.dll";
const DEFAULT_INI_NAME: &str = "ReShade.ini";
const MAX_DOWNLOAD_BYTES: u64 = 256 * 1024 * 1024;
const OFFICIAL_HOST_PREFIXES: &[&str] = &[
    "https://github.com/crosire/reshade/releases/download/",
    "https://github.com/crosire/ReShade/releases/download/",
];

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReshadeRequest {
    pub game_id: String,
    pub game_name: String,
    pub install_dir: String,
    pub executable: String,
    pub version_policy: String,
    #[serde(default)]
    pub pinned_tag: Option<String>,
    #[serde(default)]
    pub archive_url: Option<String>,
    #[serde(default)]
    pub local_archive: Option<String>,
    #[serde(default)]
    pub sha256: Option<String>,
    pub proxy: String,
    #[serde(default)]
    pub safety_notes: Vec<String>,
    /// Optional GitHub releases API URL used by `update_check`.
    /// Currently informational; the live resolver is still planned for v1.
    #[serde(default)]
    pub update_url: Option<String>,
}

/// Reports a collision between ReShade's chosen proxy DLL and another
/// loader/injector already in the game's executable directory. Moddin
/// refuses to overwrite the existing DLL even when its shape is unknown.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReshadeConflict {
    pub proxy: String,
    pub path: String,
    pub held_by: String,
    pub managed_by_moddin: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReshadePreview {
    pub can_apply: bool,
    pub executable_exists: bool,
    pub game_running: bool,
    pub installed: bool,
    pub installed_version: Option<String>,
    pub proxy_chosen: String,
    pub proxy_available: bool,
    pub archive_reachable: bool,
    pub archive_sha256: Option<String>,
    pub conflicts: Vec<ReshadeConflict>,
    pub update_available: bool,
    pub latest_version: Option<String>,
    pub changes: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReshadeResult {
    pub installed: bool,
    pub version: Option<String>,
    pub proxy: String,
    pub transaction: Option<TransactionRecord>,
    pub installed_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReshadeMarker {
    version: String,
    proxy: String,
    archive_sha256: Option<String>,
    installed_files: Vec<String>,
}

fn tool_root() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("Moddin")
        .join("tools")
        .join("reshade")
}

fn marker_path(game_id: &str) -> Result<PathBuf, String> {
    if game_id.is_empty()
        || !game_id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err("Invalid Moddin game id for ReShade storage.".to_owned());
    }
    Ok(tool_root().join(format!("{game_id}.json")))
}

fn safe_join_relative(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let path = Path::new(relative);
    if path.as_os_str().is_empty() {
        return Err("Catalog executable path is empty.".to_owned());
    }
    for component in path.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(format!("Path escapes the game directory: {relative}"));
            }
        }
    }
    Ok(root.join(path))
}

fn validate_proxy(proxy: &str) -> Result<&str, String> {
    if !proxy.to_ascii_lowercase().ends_with(".dll") || proxy.contains('/') || proxy.contains('\\') {
        return Err(format!(
            "ReShade proxy must be a bare DLL name (e.g. 'dxgi.dll'); got '{proxy}'."
        ));
    }
    Ok(proxy)
}

fn validate_request(request: &ReshadeRequest) -> Result<(), String> {
    if request.game_id.is_empty() {
        return Err("Moddin game id is required.".to_owned());
    }
    validate_proxy(&request.proxy)?;

    match request.version_policy.as_str() {
        "pinned" => {
            if request.archive_url.is_none() && request.local_archive.is_none() {
                return Err(
                    "ReShade pinned recipes must declare archiveUrl or localArchive.".to_owned(),
                );
            }
        }
        "latest" => {
            // v1 limitation: the live resolver is not implemented yet.
            return Err(
                "ReShade 'latest' resolution is not implemented in v1; pin a known archive URL."
                    .to_owned(),
            );
        }
        other => {
            return Err(format!("Unknown ReShade versionPolicy: {other}"));
        }
    }

    if let Some(sha) = &request.sha256 {
        if sha.len() != 64 || !sha.chars().all(|character| character.is_ascii_hexdigit()) {
            return Err("ReShade recipe has an invalid SHA-256 checksum.".to_owned());
        }
    }

    Ok(())
}

fn executable_directory(request: &ReshadeRequest) -> Result<PathBuf, String> {
    let install_root = PathBuf::from(&request.install_dir);
    if !install_root.is_dir() {
        return Err(format!(
            "Game install directory does not exist: {}",
            request.install_dir
        ));
    }
    let executable_path = safe_join_relative(&install_root, &request.executable)?;
    Ok(executable_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or(install_root))
}

fn is_process_running(image_name: &str) -> bool {
    crate::process::is_process_running(image_name)
}

fn executable_process_name(request: &ReshadeRequest) -> Result<String, String> {
    let install_root = PathBuf::from(&request.install_dir);
    let executable_path = safe_join_relative(&install_root, &request.executable)?;
    let name = executable_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "Could not resolve the game executable name.".to_owned())?;
    Ok(name.to_owned())
}

fn read_marker(game_id: &str) -> Option<ReshadeMarker> {
    let path = marker_path(game_id).ok()?;
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

fn marker_is_consistent(marker: &ReshadeMarker, exec_dir: &Path) -> bool {
    marker
        .installed_files
        .iter()
        .all(|relative| exec_dir.join(relative).is_file())
}

/// Inspect the executable directory for collisions with the chosen proxy
/// DLL. The OptiScaler marker (when present) tells us Moddin itself owns
/// that proxy; the existence of any other DLL by the chosen name means a
/// non-Moddin loader is already in place and must not be overwritten.
///
/// Returns the list of conflicts plus a boolean that is `false` when the
/// proxy file is missing (the proxy is available) and `true` when the
/// file is present but unowned.
fn detect_proxy_conflicts(exec_dir: &Path, proxy: &str) -> (Vec<ReshadeConflict>, bool) {
    let proxy_path = exec_dir.join(proxy);
    if !proxy_path.is_file() {
        return (Vec::new(), true);
    }

    let optiscaler_marker = exec_dir.join(OPTISCALER_MARKER_FILE);
    if optiscaler_marker.is_file() {
        if let Ok(contents) = fs::read_to_string(&optiscaler_marker) {
            if let Ok(marker) = serde_json::from_str::<serde_json::Value>(&contents) {
                if marker.get("proxyDll").and_then(|value| value.as_str())
                    == Some(proxy)
                {
                    return (
                        vec![ReshadeConflict {
                            proxy: proxy.to_owned(),
                            path: proxy_path.to_string_lossy().into_owned(),
                            held_by: "OptiScaler (managed by Moddin)".to_owned(),
                            managed_by_moddin: true,
                        }],
                        false,
                    );
                }
            }
        }
    }

    (
        vec![ReshadeConflict {
            proxy: proxy.to_owned(),
            path: proxy_path.to_string_lossy().into_owned(),
            held_by: "another loader (not managed by Moddin)".to_owned(),
            managed_by_moddin: false,
        }],
        false,
    )
}

/// Compare the locally installed ReShade version against the latest
/// release published at the recipe's `updateUrl`. Returns
/// `(available, latest_tag)` and swallows network failures so the preview
/// remains usable when offline.
async fn check_update_info(current_version: &str, update_url: &str) -> (bool, Option<String>) {
    let request = crate::updates::ModuleUpdateRequest {
        current_version: Some(current_version.to_owned()),
        update_url: Some(update_url.to_owned()),
    };
    match crate::updates::check_module_update(request).await {
        Ok(result) if result.status == "available" => (true, result.latest_version),
        Ok(result) => (false, result.latest_version),
        Err(_) => (false, None),
    }
}

async fn download_archive(url: &str) -> Result<(Vec<u8>, String), String> {
    if !OFFICIAL_HOST_PREFIXES.iter().any(|prefix| url.starts_with(prefix)) {
        return Err(format!(
            "ReShade archive URL must come from the official crosire/reshade GitHub release; got {url}."
        ));
    }

    let client = Client::builder()
        .user_agent("Moddin-Desktop/0.1 (+https://github.com/petonexus/moddin-desktop)")
        .build()
        .map_err(|error| format!("Could not create ReShade downloader: {error}"))?;

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| format!("Could not download ReShade archive: {error}"))?
        .error_for_status()
        .map_err(|error| format!("ReShade archive endpoint returned an error: {error}"))?;

    let bytes = response
        .bytes()
        .await
        .map_err(|error| format!("Could not read ReShade archive bytes: {error}"))?;
    if bytes.len() as u64 > MAX_DOWNLOAD_BYTES {
        return Err(format!(
            "ReShade archive exceeds the {MAX_DOWNLOAD_BYTES}-byte safety limit."
        ));
    }
    let digest = sha256_of_bytes(&bytes);
    Ok((bytes.to_vec(), digest))
}

fn load_local_archive(path: &Path) -> Result<(Vec<u8>, String), String> {
    if !path.is_file() {
        return Err(format!(
            "ReShade local archive does not exist: {}",
            path.display()
        ));
    }
    let mut file = File::open(path)
        .map_err(|error| format!("Could not open ReShade archive: {error}"))?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("Could not stat ReShade archive: {error}"))?;
    if metadata.len() > MAX_DOWNLOAD_BYTES {
        return Err(format!(
            "ReShade archive exceeds the {MAX_DOWNLOAD_BYTES}-byte safety limit."
        ));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.read_to_end(&mut bytes)
        .map_err(|error| format!("Could not read ReShade archive: {error}"))?;
    let digest = sha256_of_bytes(&bytes);
    Ok((bytes, digest))
}

async fn load_archive_bytes(request: &ReshadeRequest) -> Result<(Vec<u8>, String), String> {
    if let Some(local) = &request.local_archive {
        return load_local_archive(Path::new(local));
    }
    if let Some(url) = &request.archive_url {
        return download_archive(url).await;
    }
    Err("ReShade recipe has no archive source configured.".to_owned())
}

fn sha256_of_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn sanitize_member(relative: &str) -> Option<String> {
    let path = Path::new(relative);
    let mut clean_segments: Vec<String> = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => {
                clean_segments.push(value.to_string_lossy().into_owned());
            }
            Component::CurDir => {}
            _ => return None,
        }
    }
    if clean_segments.is_empty() {
        None
    } else {
        Some(clean_segments.join("/"))
    }
}

#[tauri::command]
pub async fn preview_reshade(request: ReshadeRequest) -> Result<ReshadePreview, String> {
    validate_request(&request)?;

    let exec_dir = executable_directory(&request)?;
    let proxy_path = exec_dir.join(&request.proxy);
    let marker = read_marker(&request.game_id);
    let installed = marker
        .as_ref()
        .is_some_and(|marker| marker_is_consistent(marker, &exec_dir));
    let installed_version = marker
        .as_ref()
        .filter(|marker| marker_is_consistent(marker, &exec_dir))
        .map(|marker| marker.version.clone());

    let (conflicts, proxy_available) = detect_proxy_conflicts(&exec_dir, &request.proxy);
    let has_conflict = !conflicts.is_empty();
    let proxy_available = proxy_available && !has_conflict;

    let game_running = executable_process_name(&request)
        .map(|name| is_process_running(&name))
        .unwrap_or(false);

    let mut changes = Vec::new();
    changes.push(format!(
        "Drop ReShade host as '{}' next to '{}'.",
        request.proxy, request.executable
    ));
    if request.sha256.is_some() {
        changes.push("Verify archive SHA-256 before extracting.".to_owned());
    }
    if has_conflict {
        changes.push(format!(
            "Refuse to overwrite '{}' because it is held by another loader.",
            request.proxy
        ));
    }

    let mut warnings = request.safety_notes.clone();
    if has_conflict {
        for conflict in &conflicts {
            warnings.push(format!(
                "Proxy '{}' is already occupied by {}; pick a different ReShade proxy.",
                conflict.proxy, conflict.held_by
            ));
        }
    }

    let (update_available, latest_version) = match (
        installed_version.as_deref(),
        request.update_url.as_deref(),
    ) {
        (Some(current), Some(url)) => check_update_info(current, url).await,
        _ => (false, None),
    };

    Ok(ReshadePreview {
        can_apply: !game_running && proxy_available,
        executable_exists: exec_dir.join(&request.executable).is_file(),
        game_running,
        installed,
        installed_version,
        proxy_chosen: request.proxy.clone(),
        proxy_available,
        archive_reachable: request.archive_url.is_some() || request.local_archive.is_some(),
        archive_sha256: None,
        conflicts,
        update_available,
        latest_version,
        changes,
        warnings,
    })
}

#[tauri::command]
pub async fn install_reshade(request: ReshadeRequest) -> Result<ReshadeResult, String> {
    validate_request(&request)?;

    let exec_dir = executable_directory(&request)?;
    let process_name = executable_process_name(&request)?;
    if is_process_running(&process_name) {
        return Err(format!(
            "Cannot install ReShade while '{process_name}' is running."
        ));
    }

    let (conflicts, proxy_available) = detect_proxy_conflicts(&exec_dir, &request.proxy);
    if !conflicts.is_empty() {
        let detail = conflicts
            .iter()
            .map(|conflict| format!("'{}' held by {}", conflict.proxy, conflict.held_by))
            .collect::<Vec<_>>()
            .join("; ");
        return Err(format!(
            "Cannot install ReShade: {detail}. Pick a different proxy in the catalog recipe."
        ));
    }
    if !proxy_available {
        return Err(format!(
            "Cannot install ReShade: '{}' already exists in the game directory.",
            request.proxy
        ));
    }

    let (archive_bytes, archive_sha) = load_archive_bytes(&request).await?;
    if let Some(expected) = request.sha256.as_deref() {
        let actual = sha256_of_bytes(&archive_bytes);
        if !actual.eq_ignore_ascii_case(expected) {
            return Err(format!(
                "ReShade archive SHA-256 mismatch: expected {expected}, got {actual}."
            ));
        }
    }

    let mut archive = ZipArchive::new(Cursor::new(archive_bytes.as_slice()))
        .map_err(|error| format!("Could not open ReShade archive as zip: {error}"))?;

    let host_target = exec_dir.join(&request.proxy);
    let ini_target = exec_dir.join(DEFAULT_INI_NAME);
    let addon_target = exec_dir.join(ADDON_DLL_NAME);

    let mut targets: Vec<PathBuf> = Vec::new();
    let mut installed_files: Vec<String> = Vec::new();
    let mut has_ini = false;

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("ReShade archive entry {index} is unreadable: {error}"))?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_owned();
        let Some(_relative) = sanitize_member(&name) else {
            return Err(format!("ReShade archive contains an unsafe path '{name}'."));
        };
        let lower_basename = Path::new(&name)
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.to_ascii_lowercase())
            .unwrap_or_default();

        let target = if lower_basename == HOST_DLL_NAME.to_ascii_lowercase() {
            targets.push(host_target.clone());
            host_target.clone()
        } else if lower_basename == ADDON_DLL_NAME.to_ascii_lowercase() {
            targets.push(addon_target.clone());
            addon_target.clone()
        } else if lower_basename == DEFAULT_INI_NAME.to_ascii_lowercase() {
            targets.push(ini_target.clone());
            ini_target.clone()
        } else {
            // Skip non-ReShade files (readmes, etc.).
            continue;
        };

        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Could not create directory for ReShade install: {error}"))?;
        }

        let mut buffer = Vec::new();
        entry
            .read_to_end(&mut buffer)
            .map_err(|error| format!("Could not read ReShade archive entry '{name}': {error}"))?;

        let mut file = File::create(&target)
            .map_err(|error| format!("Could not write ReShade file '{}': {error}", target.display()))?;
        file.write_all(&buffer)
            .map_err(|error| format!("Could not write ReShade file '{}': {error}", target.display()))?;
        file.sync_all()
            .map_err(|error| format!("Could not flush ReShade file: {error}"))?;

        let written_name = target
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or(name);
        installed_files.push(written_name);
        if target == ini_target {
            has_ini = true;
        }
    }

    if !installed_files
        .iter()
        .any(|name| name.eq_ignore_ascii_case(&request.proxy))
    {
        return Err(format!(
            "ReShade archive did not contain a host DLL ('{HOST_DLL_NAME}')."
        ));
    }

    if !has_ini {
        let default_ini = b"[GENERAL]\r\nEffectSearchPaths=./ReShade\r\n";
        let mut file = File::create(&ini_target)
            .map_err(|error| format!("Could not write ReShade default INI: {error}"))?;
        file.write_all(default_ini)
            .map_err(|error| format!("Could not write ReShade default INI: {error}"))?;
        targets.push(ini_target.clone());
        installed_files.push(DEFAULT_INI_NAME.to_owned());
    }

    let mut metadata = BTreeMap::new();
    metadata.insert("archive_sha256".to_owned(), archive_sha.clone());
    metadata.insert("proxy".to_owned(), request.proxy.clone());
    metadata.insert("processName".to_owned(), process_name);

    let record = transaction::begin_file_set_transaction(
        &exec_dir,
        &targets,
        "reshade",
        &format!("Install ReShade host for {}", request.game_name),
        &request.game_id,
        metadata,
    )?;
    let record = transaction::mark_applied(record)?;

    let marker = ReshadeMarker {
        version: request
            .pinned_tag
            .clone()
            .unwrap_or_else(|| "pinned".to_owned()),
        proxy: request.proxy.clone(),
        archive_sha256: Some(archive_sha),
        installed_files: installed_files.clone(),
    };
    fs::write(
        marker_path(&request.game_id)?,
        serde_json::to_vec_pretty(&marker)
            .map_err(|error| format!("Could not serialize ReShade marker: {error}"))?,
    )
    .map_err(|error| format!("Could not write ReShade marker: {error}"))?;

    Ok(ReshadeResult {
        installed: true,
        version: Some(marker.version),
        proxy: request.proxy.clone(),
        transaction: Some(record),
        installed_files,
    })
}

#[tauri::command]
pub async fn uninstall_reshade(game_id: String, transaction_id: Option<String>) -> Result<TransactionRecord, String> {
    let record = match transaction_id {
        Some(id) => transaction::rollback_transaction(id)?,
        None => transaction::rollback_latest_module_transaction(game_id.clone(), "reshade".to_owned())?,
    };
    let marker = marker_path(&game_id)?;
    if marker.is_file() {
        let _ = fs::remove_file(&marker);
    }
    Ok(record)
}

#[allow(dead_code)]
fn _marker_marker_path(game_id: &str) -> Result<PathBuf, String> {
    // silence dead-code lint for the marker file constant until a second consumer appears
    let _ = MARKER_FILE;
    marker_path(game_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_proxy_name() {
        assert!(validate_proxy("dxgi.dll").is_ok());
        assert!(validate_proxy("version.dll").is_ok());
        assert!(validate_proxy("sub/dxgi.dll").is_err());
        assert!(validate_proxy("dxgi").is_err());
        assert!(validate_proxy("").is_err());
    }

    #[test]
    fn validates_version_policy() {
        let mut request = base_request();
        request.version_policy = "latest".to_owned();
        assert!(validate_request(&request).is_err());

        request.version_policy = "pinned".to_owned();
        request.archive_url = Some("https://github.com/crosire/reshade/releases/download/v6.0.0/ReShade.zip".to_owned());
        request.sha256 = Some("0".repeat(64));
        assert!(validate_request(&request).is_ok());

        request.sha256 = Some("not-a-digest".to_owned());
        assert!(validate_request(&request).is_err());
    }

    #[test]
    fn sanitizes_archive_member_paths() {
        assert_eq!(sanitize_member("ReShade64.dll").as_deref(), Some("ReShade64.dll"));
        assert_eq!(
            sanitize_member("./ReShade/ReShade64.dll").as_deref(),
            Some("ReShade/ReShade64.dll")
        );
        assert_eq!(sanitize_member("../escape.dll"), None);
        assert_eq!(sanitize_member(""), None);
    }

    fn base_request() -> ReshadeRequest {
        ReshadeRequest {
            game_id: "test".to_owned(),
            game_name: "Test".to_owned(),
            install_dir: String::new(),
            executable: "test.exe".to_owned(),
            version_policy: "pinned".to_owned(),
            pinned_tag: None,
            archive_url: None,
            local_archive: None,
            sha256: None,
            proxy: "dxgi.dll".to_owned(),
            safety_notes: Vec::new(),
            update_url: None,
        }
    }
}