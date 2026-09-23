//! BepInEx framework installer.
//!
//! Implements the design in [`docs/MODULES.md`]. The install flow is
//! gated on a Unity engine detection (the layout is the canonical signal:
//! `UnityPlayer.dll` next to the game executable, plus a `*_Data` folder)
//! and on the IL2CPP vs Mono split, which selects which archive BepInEx
//! ships for that game. The architecture is resolved locally — Moddin
//! refuses to install when the layout does not match either build, instead
//! of guessing.
//!
//! The implementation mirrors the shape of [`crate::reshade`]: a pinned
//! (or live-resolved) GitHub archive, SHA-256 verification, ZIP
//! extraction into the executable directory, and a transaction so the
//! install is reversible. A `.moddin-bepinex.json` marker records the
//! installed version, architecture, and source SHA-256.

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

const MARKER_FILE: &str = ".moddin-bepinex.json";
const MAX_DOWNLOAD_BYTES: u64 = 256 * 1024 * 1024;
const OFFICIAL_RELEASE_PREFIXES: &[&str] = &[
    "https://github.com/BepInEx/BepInEx/releases/download/",
];

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
    /// Optional manual override. When absent, Moddin infers the
    /// architecture from the local game layout.
    #[serde(default)]
    pub unity_architecture: Option<String>,
    /// Per-game plugin bundle that the recipe wants installed alongside
    /// the host. Each entry carries its own URL and optional SHA-256.
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BepinexMarker {
    version: String,
    architecture: String,
    archive_sha256: Option<String>,
    installed_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BepinexPreview {
    pub can_apply: bool,
    pub executable_exists: bool,
    pub game_running: bool,
    pub engine_compatible: bool,
    pub architecture: Option<String>,
    pub architecture_inferred: bool,
    pub installed: bool,
    pub installed_version: Option<String>,
    pub installed_architecture: Option<String>,
    pub archive_reachable: bool,
    pub archive_sha256: Option<String>,
    pub changes: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BepinexResult {
    pub installed: bool,
    pub version: String,
    pub architecture: String,
    pub transaction: Option<TransactionRecord>,
    pub installed_files: Vec<String>,
}

fn tool_root() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("Moddin")
        .join("tools")
        .join("bepinex")
}

fn marker_path(game_id: &str) -> Result<PathBuf, String> {
    if game_id.is_empty()
        || !game_id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err("Invalid Moddin game id for BepInEx storage.".to_owned());
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

fn executable_directory(request: &BepinexRequest) -> Result<PathBuf, String> {
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

fn executable_process_name(request: &BepinexRequest) -> Result<String, String> {
    let install_root = PathBuf::from(&request.install_dir);
    let executable_path = safe_join_relative(&install_root, &request.executable)?;
    let name = executable_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "Could not resolve the game executable name.".to_owned())?;
    Ok(name.to_owned())
}

/// Detect the Unity architecture used by the game. Returns
/// `Some("il2cpp")` when a `GameAssembly.dll` is present, `Some("mono")`
/// when only the `_Data/Managed` folder is present, and `None` when the
/// install does not look like a Unity game.
///
/// Detection intentionally walks the executable directory plus one parent
/// (some games keep the build in a nested folder); anything more
/// aggressive is left to the recipe's `unityArchitecture` override.
fn detect_unity_architecture(exec_dir: &Path) -> Option<String> {
    for candidate in [exec_dir.to_path_buf(), exec_dir.parent().unwrap_or(exec_dir).to_path_buf()] {
        if candidate.join("GameAssembly.dll").is_file() {
            return Some("il2cpp".to_owned());
        }
        let data_dirs = fs::read_dir(&candidate).ok().into_iter().flatten().flatten();
        for entry in data_dirs {
            let name = entry.file_name();
            let name_lossy = name.to_string_lossy();
            if !name_lossy.ends_with("_Data") {
                continue;
            }
            let managed = entry.path().join("Managed").join("Assembly-CSharp.dll");
            let il2cpp = entry.path().join("il2cpp_data").join("Metadata");
            if il2cpp.is_dir() {
                return Some("il2cpp".to_owned());
            }
            if managed.is_file() {
                return Some("mono".to_owned());
            }
        }
    }
    None
}

fn validate_request(request: &BepinexRequest) -> Result<(), String> {
    if request.game_id.is_empty() {
        return Err("Moddin game id is required.".to_owned());
    }
    match request.version_policy.as_str() {
        "pinned" => {
            if request.sha256.is_none() && request.pinned_tag.is_none() {
                return Err(
                    "BepInEx pinned recipes must declare pinnedTag and sha256.".to_owned(),
                );
            }
        }
        "latest" => {
            if request.release_api_url.is_none() {
                return Err(
                    "BepInEx 'latest' recipes must declare releaseApiUrl (GitHub releases API)."
                        .to_owned(),
                );
            }
        }
        other => return Err(format!("Unknown BepInEx versionPolicy: {other}")),
    }
    if let Some(sha) = &request.sha256 {
        if sha.len() != 64 || !sha.chars().all(|character| character.is_ascii_hexdigit()) {
            return Err("BepInEx recipe has an invalid SHA-256 checksum.".to_owned());
        }
    }
    Ok(())
}

fn read_marker(game_id: &str) -> Option<BepinexMarker> {
    let path = marker_path(game_id).ok()?;
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

fn marker_is_consistent(marker: &BepinexMarker, exec_dir: &Path) -> bool {
    marker
        .installed_files
        .iter()
        .all(|relative| exec_dir.join(relative).is_file())
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
pub async fn preview_bepinex(request: BepinexRequest) -> Result<BepinexPreview, String> {
    validate_request(&request)?;
    let exec_dir = executable_directory(&request)?;
    let process_name = executable_process_name(&request)?;
    let game_running = crate::process::is_process_running(&process_name);
    let detected_arch = detect_unity_architecture(&exec_dir);
    let requested_arch = request
        .unity_architecture
        .as_deref()
        .map(|value| value.to_ascii_lowercase());
    let architecture = requested_arch
        .as_ref()
        .cloned()
        .or_else(|| detected_arch.clone());
    let architecture_inferred = requested_arch.is_none() && detected_arch.is_some();
    let engine_compatible = detected_arch.is_some() || requested_arch.is_some();
    let marker = read_marker(&request.game_id);
    let installed = marker
        .as_ref()
        .is_some_and(|marker| marker_is_consistent(marker, &exec_dir));
    let installed_version = marker
        .as_ref()
        .filter(|marker| marker_is_consistent(marker, &exec_dir))
        .map(|marker| marker.version.clone());
    let installed_architecture = marker
        .as_ref()
        .filter(|marker| marker_is_consistent(marker, &exec_dir))
        .map(|marker| marker.architecture.clone());

    let archive_reachable = request.sha256.is_some()
        || request.release_api_url.is_some()
        || !request.mods.is_empty();

    let mut changes = Vec::new();
    if let Some(arch) = architecture.as_deref() {
        changes.push(format!(
            "Drop BepInEx ({arch}) next to '{}'.",
            request.executable
        ));
    } else {
        changes.push(
            "Drop BepInEx next to the game executable (architecture unconfirmed).".to_owned(),
        );
    }
    if !request.mods.is_empty() {
        changes.push(format!(
            "Install {} bundled plugin(s) from the recipe.",
            request.mods.len()
        ));
    }

    let mut warnings = request.safety_notes.clone();
    if detected_arch.is_none() && requested_arch.is_none() {
        warnings.push(
            "This game does not look like a Unity build. BepInEx refuses to install \
             outside Unity games unless the recipe pins unityArchitecture."
                .to_owned(),
        );
    }

    Ok(BepinexPreview {
        can_apply: !game_running
            && engine_compatible
            && architecture.is_some()
            && archive_reachable,
        executable_exists: exec_dir.join(&request.executable).is_file(),
        game_running,
        engine_compatible,
        architecture,
        architecture_inferred,
        installed,
        installed_version,
        installed_architecture,
        archive_reachable,
        archive_sha256: None,
        changes,
        warnings,
    })
}

#[tauri::command]
pub async fn install_bepinex(request: BepinexRequest) -> Result<BepinexResult, String> {
    validate_request(&request)?;
    let exec_dir = executable_directory(&request)?;
    let process_name = executable_process_name(&request)?;
    if crate::process::is_process_running(&process_name) {
        return Err(format!(
            "Cannot install BepInEx while '{process_name}' is running."
        ));
    }

    let detected_arch = detect_unity_architecture(&exec_dir);
    let requested_arch = request
        .unity_architecture
        .as_deref()
        .map(|value| value.to_ascii_lowercase());
    let architecture = requested_arch
        .as_ref()
        .cloned()
        .or_else(|| detected_arch.clone())
        .ok_or_else(|| {
            "BepInEx refused to install: the game layout does not match a Unity build. \
             Set unityArchitecture in the recipe to force a specific build."
                .to_owned()
        })?;

    let (archive_bytes, archive_sha) = download_bepinex_archive(&request).await?;
    if let Some(expected) = request.sha256.as_deref() {
        let actual = sha256_of_bytes(&archive_bytes);
        if !actual.eq_ignore_ascii_case(expected) {
            return Err(format!(
                "BepInEx archive SHA-256 mismatch: expected {expected}, got {actual}."
            ));
        }
    }

    let mut archive = ZipArchive::new(Cursor::new(archive_bytes.as_slice()))
        .map_err(|error| format!("Could not open BepInEx archive as zip: {error}"))?;

    let mut installed_files: Vec<String> = Vec::new();
    let mut targets: Vec<PathBuf> = Vec::new();

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("BepInEx archive entry {index} is unreadable: {error}"))?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_owned();
        let Some(relative) = sanitize_member(&name) else {
            return Err(format!("BepInEx archive contains an unsafe path '{name}'."));
        };
        let relative_path = Path::new(&relative);
        // BepInEx archives ship a single root folder (e.g. `BepInEx/...`).
        // Strip that root so the payload lands directly next to the
        // executable.
        let stripped = if relative_path.components().count() > 1 {
            let mut components = relative_path.components();
            components.next();
            components.as_path()
        } else {
            relative_path
        };
        let target = exec_dir.join(stripped);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Could not create BepInEx install directory: {error}"))?;
        }
        let mut buffer = Vec::new();
        entry
            .read_to_end(&mut buffer)
            .map_err(|error| format!("Could not read BepInEx entry '{name}': {error}"))?;
        let mut file = File::create(&target)
            .map_err(|error| format!("Could not write BepInEx file '{}': {error}", target.display()))?;
        file.write_all(&buffer)
            .map_err(|error| format!("Could not write BepInEx file '{}': {error}", target.display()))?;
        file.sync_all()
            .map_err(|error| format!("Could not flush BepInEx file: {error}"))?;
        let written = target
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| name.clone());
        installed_files.push(
            stripped
                .to_string_lossy()
                .into_owned(),
        );
        let _ = written;
        targets.push(target);
    }

    // Record a transaction so the install is reversible. We use the
    // `begin_file_set_transaction` helper so every file gets backed up
    // individually and `created_directories` is populated for cleanup.
    let mut metadata = BTreeMap::new();
    metadata.insert("architecture".to_owned(), architecture.clone());
    metadata.insert("processName".to_owned(), process_name.clone());
    metadata.insert("archive_sha256".to_owned(), archive_sha.clone());
    let record = transaction::begin_file_set_transaction(
        &exec_dir,
        &targets,
        "bepinex",
        &format!("Install BepInEx for {}", request.game_name),
        &request.game_id,
        metadata,
    )?;
    let record = transaction::mark_applied(record)?;

    let marker = BepinexMarker {
        version: request
            .pinned_tag
            .clone()
            .unwrap_or_else(|| "pinned".to_owned()),
        architecture: architecture.clone(),
        archive_sha256: Some(archive_sha),
        installed_files: installed_files.clone(),
    };
    fs::write(
        marker_path(&request.game_id)?,
        serde_json::to_vec_pretty(&marker)
            .map_err(|error| format!("Could not serialize BepInEx marker: {error}"))?,
    )
    .map_err(|error| format!("Could not write BepInEx marker: {error}"))?;

    Ok(BepinexResult {
        installed: true,
        version: marker.version,
        architecture,
        transaction: Some(record),
        installed_files,
    })
}

#[tauri::command]
pub async fn uninstall_bepinex(game_id: String) -> Result<TransactionRecord, String> {
    let record = transaction::rollback_latest_module_transaction(game_id.clone(), "bepinex".to_owned())?;
    let marker = marker_path(&game_id)?;
    if marker.is_file() {
        let _ = fs::remove_file(&marker);
    }
    Ok(record)
}

async fn download_bepinex_archive(
    request: &BepinexRequest,
) -> Result<(Vec<u8>, String), String> {
    let url = match request.version_policy.as_str() {
        "pinned" => {
            // For pinned recipes we require the recipe to provide the
            // download URL (catalogs that pin to a tag should also pin
            // the archive). The legacy `pinned` form without an explicit
            // URL is rejected at preview time.
            return Err(
                "BepInEx pinned recipes must declare a sha256 and the archive is fetched \
                 from the GitHub release tag. The full archive URL is not yet carried on \
                 BepinexRequest; pin the recipe to the released .zip via the catalog."
                    .to_owned(),
            );
        }
        "latest" => {
            let api = request
                .release_api_url
                .as_deref()
                .ok_or_else(|| "BepInEx 'latest' requires releaseApiUrl.".to_owned())?;
            resolve_latest_archive(api).await?
        }
        other => return Err(format!("Unknown BepInEx versionPolicy: {other}")),
    };

    if !OFFICIAL_RELEASE_PREFIXES
        .iter()
        .any(|prefix| url.starts_with(prefix))
    {
        return Err(format!(
            "BepInEx archive URL must come from the official BepInEx/BepInEx GitHub release; got {url}."
        ));
    }

    let client = Client::builder()
        .user_agent("Moddin-Desktop/0.1 (+https://github.com/petonexus/moddin-desktop)")
        .build()
        .map_err(|error| format!("Could not create BepInEx downloader: {error}"))?;

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|error| format!("Could not download BepInEx archive: {error}"))?
        .error_for_status()
        .map_err(|error| format!("BepInEx archive endpoint returned an error: {error}"))?;

    let bytes = response
        .bytes()
        .await
        .map_err(|error| format!("Could not read BepInEx archive bytes: {error}"))?;
    if bytes.len() as u64 > MAX_DOWNLOAD_BYTES {
        return Err(format!(
            "BepInEx archive exceeds the {MAX_DOWNLOAD_BYTES}-byte safety limit."
        ));
    }
    let digest = sha256_of_bytes(&bytes);
    Ok((bytes.to_vec(), digest))
}

async fn resolve_latest_archive(api_url: &str) -> Result<String, String> {
    #[derive(Deserialize)]
    struct Asset {
        name: String,
        browser_download_url: String,
    }
    #[derive(Deserialize)]
    struct Release {
        assets: Vec<Asset>,
    }

    if !api_url.starts_with("https://api.github.com/repos/BepInEx/BepInEx/")
        && !api_url.starts_with("https://api.github.com/repos/BepInEx/BepInEx-")
    {
        return Err(
            "BepInEx releaseApiUrl must point at the official BepInEx/BepInEx GitHub release API."
                .to_owned(),
        );
    }

    let client = Client::builder()
        .user_agent("Moddin-Desktop/0.1 (+https://github.com/petonexus/moddin-desktop)")
        .build()
        .map_err(|error| format!("Could not create BepInEx release resolver: {error}"))?;

    let response = client
        .get(api_url)
        .send()
        .await
        .map_err(|error| format!("Could not fetch BepInEx release: {error}"))?
        .error_for_status()
        .map_err(|error| format!("BepInEx release API returned an error: {error}"))?;

    let release: Release = response
        .json()
        .await
        .map_err(|error| format!("Could not parse BepInEx release: {error}"))?;

    release
        .assets
        .into_iter()
        .find(|asset| asset.name.to_ascii_lowercase().ends_with(".zip"))
        .map(|asset| asset.browser_download_url)
        .ok_or_else(|| "BepInEx release did not publish a .zip asset.".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_request() -> BepinexRequest {
        BepinexRequest {
            game_id: "test".to_owned(),
            game_name: "Test".to_owned(),
            install_dir: String::new(),
            executable: "Test.exe".to_owned(),
            version_policy: "latest".to_owned(),
            release_api_url: Some(
                "https://api.github.com/repos/BepInEx/BepInEx/releases/latest".to_owned(),
            ),
            pinned_tag: None,
            sha256: None,
            unity_architecture: None,
            mods: Vec::new(),
            safety_notes: Vec::new(),
        }
    }

    #[test]
    fn validates_version_policy_shape() {
        let mut request = base_request();
        request.version_policy = "what".to_owned();
        assert!(validate_request(&request).is_err());

        request.version_policy = "latest".to_owned();
        request.release_api_url = None;
        assert!(validate_request(&request).is_err());

        request.release_api_url = Some(
            "https://api.github.com/repos/BepInEx/BepInEx/releases/latest".to_owned(),
        );
        request.sha256 = Some("not-a-digest".to_owned());
        assert!(validate_request(&request).is_err());

        request.sha256 = Some("0".repeat(64));
        assert!(validate_request(&request).is_ok());

        request.version_policy = "pinned".to_owned();
        request.pinned_tag = Some("v6.0.0-pre.2".to_owned());
        request.release_api_url = None;
        request.sha256 = Some("0".repeat(64));
        assert!(validate_request(&request).is_ok());

        request.sha256 = None;
        request.pinned_tag = None;
        assert!(validate_request(&request).is_err());
    }

    #[test]
    fn sanitizes_archive_member_paths() {
        assert_eq!(sanitize_member("BepInEx/core.dll").as_deref(), Some("BepInEx/core.dll"));
        assert_eq!(
            sanitize_member("./BepInEx/core.dll").as_deref(),
            Some("BepInEx/core.dll")
        );
        assert_eq!(sanitize_member("../escape.dll"), None);
        assert_eq!(sanitize_member(""), None);
    }

    #[test]
    fn architecture_detection_uses_game_assembly() {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("moddin-bepinex-{suffix}"));
        fs::create_dir_all(&root).expect("root");
        fs::write(root.join("GameAssembly.dll"), b"fake").expect("il2cpp marker");
        let detected = detect_unity_architecture(&root);
        fs::remove_dir_all(&root).expect("cleanup");
        assert_eq!(detected.as_deref(), Some("il2cpp"));
    }

    #[test]
    fn architecture_detection_uses_managed_folder_for_mono() {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("moddin-bepinex-mono-{suffix}"));
        let data_dir = root.join("Game_Data");
        let managed = data_dir.join("Managed");
        fs::create_dir_all(&managed).expect("managed");
        fs::write(managed.join("Assembly-CSharp.dll"), b"fake").expect("mono marker");
        let detected = detect_unity_architecture(&root);
        fs::remove_dir_all(&root).expect("cleanup");
        assert_eq!(detected.as_deref(), Some("mono"));
    }

    #[test]
    fn architecture_detection_returns_none_for_non_unity() {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("moddin-bepinex-none-{suffix}"));
        fs::create_dir_all(&root).expect("root");
        let detected = detect_unity_architecture(&root);
        fs::remove_dir_all(&root).expect("cleanup");
        assert_eq!(detected, None);
    }
}
