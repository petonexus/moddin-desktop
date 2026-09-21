use crate::transaction::{self, TransactionRecord};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Component, Path, PathBuf},
    time::Duration,
};

const MARKER_FILE: &str = ".moddin-cheeky-foveated-dlss.json";
const OFFICIAL_RELEASE_PREFIX: &str =
    "https://github.com/ClarkCheekyKent/CheekyFoveatedDLSS/releases/download/";

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheekyFoveatedDlssRequest {
    pub game_id: String,
    pub game_name: String,
    pub install_dir: String,
    pub executable: String,
    pub version: String,
    pub download_url: String,
    pub sha256: String,
    pub addon_file: String,
    #[serde(default)]
    pub safety_notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheekyFoveatedDlssPreview {
    pub can_apply: bool,
    pub game_running: bool,
    pub executable_exists: bool,
    pub executable_path: String,
    pub executable_directory: String,
    pub addon_path: String,
    pub installed: bool,
    pub installed_version: Option<String>,
    pub manual_install_detected: bool,
    pub openxr_setup_required: bool,
    pub changes: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CheekyMarker {
    version: String,
    source_sha256: String,
    installed_files: Vec<String>,
}

fn safe_join_relative(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let relative_path = Path::new(relative);
    if relative_path.as_os_str().is_empty() {
        return Err("Cheeky Foveated DLSS add-on path is empty.".to_owned());
    }

    for component in relative_path.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(format!(
                    "Catalog path must stay inside the game directory: {relative}"
                ));
            }
        }
    }

    Ok(root.join(relative_path))
}

fn validate_request(request: &CheekyFoveatedDlssRequest) -> Result<(), String> {
    if request.game_id.trim().is_empty() || request.game_name.trim().is_empty() {
        return Err("Cheeky Foveated DLSS recipe must identify the game.".to_owned());
    }

    if !request.download_url.starts_with(OFFICIAL_RELEASE_PREFIX) {
        return Err(
            "Cheeky Foveated DLSS downloads must come from the official GitHub release.".to_owned(),
        );
    }

    let sha = request.sha256.trim();
    if sha.len() != 64 || !sha.chars().all(|character| character.is_ascii_hexdigit()) {
        return Err("Cheeky Foveated DLSS recipe has an invalid SHA-256 checksum.".to_owned());
    }

    if request.version.trim().is_empty() {
        return Err("Cheeky Foveated DLSS recipe has no release version.".to_owned());
    }

    let addon_name = Path::new(&request.addon_file);
    if addon_name.components().count() != 1
        || !request
            .addon_file
            .to_ascii_lowercase()
            .ends_with(".addon64")
    {
        return Err(format!(
            "Invalid Cheeky Foveated DLSS add-on filename: {}",
            request.addon_file
        ));
    }
    safe_join_relative(Path::new("."), &request.addon_file)?;

    Ok(())
}

fn executable_context(
    request: &CheekyFoveatedDlssRequest,
) -> Result<(PathBuf, PathBuf, String), String> {
    let install_root = PathBuf::from(&request.install_dir);
    if !install_root.is_dir() {
        return Err(format!(
            "Game install directory does not exist: {}",
            request.install_dir
        ));
    }

    let executable_path = safe_join_relative(&install_root, &request.executable)?;
    let executable_directory = executable_path
        .parent()
        .ok_or_else(|| "Could not resolve the game executable directory.".to_owned())?
        .to_path_buf();
    let process_name = executable_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "Could not resolve the game executable name.".to_owned())?
        .to_owned();

    Ok((executable_path, executable_directory, process_name))
}

fn is_process_running(image_name: &str) -> bool {
    crate::process::is_process_running(image_name)
}

fn marker_path(executable_directory: &Path) -> PathBuf {
    executable_directory.join(MARKER_FILE)
}

fn read_marker(executable_directory: &Path) -> Option<CheekyMarker> {
    let contents = fs::read_to_string(marker_path(executable_directory)).ok()?;
    serde_json::from_str(&contents).ok()
}

fn preview_cheeky_foveated_dlss_sync(
    request: CheekyFoveatedDlssRequest,
) -> Result<CheekyFoveatedDlssPreview, String> {
    validate_request(&request)?;
    let (executable_path, executable_directory, process_name) = executable_context(&request)?;
    let addon_path = safe_join_relative(&executable_directory, &request.addon_file)?;
    let marker = read_marker(&executable_directory);
    let game_running = is_process_running(&process_name);
    let manual_install_detected = marker.is_none() && addon_path.is_file();
    let installed = marker.as_ref().is_some_and(|value| {
        value.installed_files.iter().all(|relative| {
            safe_join_relative(&executable_directory, relative).is_ok_and(|path| path.is_file())
        })
    });

    let mut changes = vec![format!(
        "Download Cheeky Foveated DLSS {} from the official GitHub release and verify SHA-256 {}.",
        request.version, request.sha256
    )];
    if installed {
        changes.push(format!(
            "Update the Moddin-managed {} beside the game executable.",
            request.addon_file
        ));
    } else {
        changes.push(format!(
            "Install {} beside the game executable and record it for rollback.",
            request.addon_file
        ));
    }
    changes.push(
        "Back up any Moddin-managed file before replacing it and verify the written checksum."
            .to_owned(),
    );

    let mut warnings = request.safety_notes.clone();
    if warnings.is_empty() {
        warnings.extend([
            "Requires 64-bit ReShade with full add-on support installed for the game's D3D11/D3D12 API.",
            "Do not use the Cheeky ReShade add-on and the Cheeky UEVR plugin in the same game.",
            "For OpenXR VR, run CheekyOpenXRSetup.exe from the matching release once; Moddin does not run that third-party installer automatically.",
        ].map(str::to_owned));
    }
    if game_running {
        warnings.push(format!(
            "{process_name} is running. Close the game before installing or updating the add-on."
        ));
    }
    if manual_install_detected {
        warnings.push(
            "The add-on file already exists without a Moddin marker. Automatic overwrite is blocked to protect the existing setup."
                .to_owned(),
        );
    }

    Ok(CheekyFoveatedDlssPreview {
        can_apply: executable_path.is_file() && !game_running && !manual_install_detected,
        game_running,
        executable_exists: executable_path.is_file(),
        executable_path: executable_path.to_string_lossy().into_owned(),
        executable_directory: executable_directory.to_string_lossy().into_owned(),
        addon_path: addon_path.to_string_lossy().into_owned(),
        installed,
        installed_version: marker.map(|value| value.version),
        manual_install_detected,
        openxr_setup_required: true,
        changes,
        warnings,
    })
}

#[tauri::command]
pub async fn preview_cheeky_foveated_dlss(
    request: CheekyFoveatedDlssRequest,
) -> Result<CheekyFoveatedDlssPreview, String> {
    tauri::async_runtime::spawn_blocking(move || preview_cheeky_foveated_dlss_sync(request))
        .await
        .map_err(|error| format!("Cheeky preview task failed: {error}"))?
}

fn install_from_bytes(
    request: CheekyFoveatedDlssRequest,
    bytes: Vec<u8>,
) -> Result<TransactionRecord, String> {
    let preview = preview_cheeky_foveated_dlss_sync(request.clone())?;
    if !preview.can_apply {
        return Err(
            "Cheeky Foveated DLSS installation is blocked by the current game environment."
                .to_owned(),
        );
    }

    let actual_hash = format!("{:x}", Sha256::digest(&bytes));
    if !actual_hash.eq_ignore_ascii_case(request.sha256.trim()) {
        return Err(format!(
            "Cheeky Foveated DLSS SHA-256 mismatch. Expected {}, received {}. Nothing was installed.",
            request.sha256, actual_hash
        ));
    }

    let (_, executable_directory, process_name) = executable_context(&request)?;
    let addon_target = safe_join_relative(&executable_directory, &request.addon_file)?;
    let marker_target = marker_path(&executable_directory);
    let mut metadata = BTreeMap::new();
    metadata.insert("processName".to_owned(), process_name);
    metadata.insert("version".to_owned(), request.version.clone());
    metadata.insert("sourceSha256".to_owned(), actual_hash.clone());
    metadata.insert("integration".to_owned(), "reshade-addon".to_owned());

    let transaction = transaction::begin_file_set_transaction(
        &executable_directory,
        &[addon_target.clone(), marker_target.clone()],
        "cheeky-foveated-dlss",
        &format!(
            "Install Cheeky Foveated DLSS {} for {}",
            request.version, request.game_name
        ),
        &request.game_id,
        metadata,
    )?;

    let apply_result = (|| -> Result<(), String> {
        fs::write(&addon_target, &bytes).map_err(|error| {
            format!(
                "Could not install '{}' to '{}': {error}",
                request.addon_file,
                addon_target.display()
            )
        })?;

        let marker = CheekyMarker {
            version: request.version.clone(),
            source_sha256: actual_hash.clone(),
            installed_files: vec![request.addon_file.clone(), MARKER_FILE.to_owned()],
        };
        let marker_json = serde_json::to_string_pretty(&marker)
            .map_err(|error| format!("Could not serialize Cheeky marker: {error}"))?;
        fs::write(&marker_target, marker_json)
            .map_err(|error| format!("Could not write Moddin Cheeky marker: {error}"))?;

        let written_hash = format!(
            "{:x}",
            Sha256::digest(
                fs::read(&addon_target)
                    .map_err(|error| format!("Could not verify installed add-on: {error}"))?,
            )
        );
        if !written_hash.eq_ignore_ascii_case(&actual_hash) {
            return Err("Cheeky add-on failed post-install checksum verification.".to_owned());
        }

        Ok(())
    })();

    if let Err(error) = apply_result {
        return match transaction::restore_record(transaction) {
            Ok(_) => Err(format!(
                "{error} All changed files were restored automatically."
            )),
            Err(restore_error) => Err(format!(
                "{error} Automatic rollback also failed: {restore_error}"
            )),
        };
    }

    transaction::mark_applied(transaction)
}

#[tauri::command]
pub async fn install_cheeky_foveated_dlss(
    request: CheekyFoveatedDlssRequest,
) -> Result<TransactionRecord, String> {
    validate_request(&request)?;
    let preview = preview_cheeky_foveated_dlss_sync(request.clone())?;
    if !preview.can_apply {
        return Err(
            "Cheeky Foveated DLSS preview reports that installation is currently blocked."
                .to_owned(),
        );
    }

    let client = reqwest::Client::builder()
        .user_agent("Moddin-Desktop/0.1 (+https://github.com/petonexus/moddin-desktop)")
        .connect_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(|error| format!("Could not create secure download client: {error}"))?;

    let response = client
        .get(&request.download_url)
        .send()
        .await
        .map_err(|error| format!("Could not download Cheeky Foveated DLSS from GitHub: {error}"))?
        .error_for_status()
        .map_err(|error| {
            format!("GitHub returned an error while downloading Cheeky Foveated DLSS: {error}")
        })?;
    let bytes = response
        .bytes()
        .await
        .map_err(|error| format!("Could not read Cheeky Foveated DLSS download: {error}"))?
        .to_vec();

    tauri::async_runtime::spawn_blocking(move || install_from_bytes(request, bytes))
        .await
        .map_err(|error| format!("Cheeky Foveated DLSS installer task failed: {error}"))?
}

#[tauri::command]
pub fn uninstall_cheeky_foveated_dlss(
    request: CheekyFoveatedDlssRequest,
) -> Result<TransactionRecord, String> {
    validate_request(&request)?;
    let (_, executable_directory, process_name) = executable_context(&request)?;
    if is_process_running(&process_name) {
        return Err(format!(
            "Close {process_name} before uninstalling Cheeky Foveated DLSS."
        ));
    }

    let target_root = executable_directory.to_string_lossy().into_owned();
    let mut last_rollback = None;
    while marker_path(&executable_directory).is_file() {
        let record = transaction::list_transactions_sync()?
            .into_iter()
            .find(|record| {
                record.status == "applied"
                    && record.kind == "cheeky-foveated-dlss"
                    && record.game_id == request.game_id
                    && record.target_path == target_root
            })
            .ok_or_else(|| {
                "The managed Cheeky marker exists, but its rollback transaction could not be found."
                    .to_owned()
            })?;
        last_rollback = Some(transaction::rollback_transaction(record.id)?);
    }

    last_rollback
        .ok_or_else(|| "No Moddin-managed Cheeky Foveated DLSS installation was found.".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> CheekyFoveatedDlssRequest {
        CheekyFoveatedDlssRequest {
            game_id: "example".to_owned(),
            game_name: "Example".to_owned(),
            install_dir: r"C:\Games\Example".to_owned(),
            executable: "game.exe".to_owned(),
            version: "0.3.4".to_owned(),
            download_url: format!("{OFFICIAL_RELEASE_PREFIX}v0.3.4/CheekyFoveatedDLSS.addon64"),
            sha256: "ad959b083b261172168c908791438cbb21eb183572ee50ec77de278580720df3".to_owned(),
            addon_file: "CheekyFoveatedDLSS.addon64".to_owned(),
            safety_notes: Vec::new(),
        }
    }

    #[test]
    fn validates_official_recipe() {
        assert!(validate_request(&request()).is_ok());
    }

    #[test]
    fn refuses_unofficial_download_hosts() {
        let mut value = request();
        value.download_url = "https://example.com/CheekyFoveatedDLSS.addon64".to_owned();
        assert!(validate_request(&value).is_err());
    }

    #[test]
    fn refuses_addon_path_traversal() {
        let mut value = request();
        value.addon_file = r"..\CheekyFoveatedDLSS.addon64".to_owned();
        assert!(validate_request(&value).is_err());
    }
}
