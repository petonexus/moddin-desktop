use crate::{
    inspection::{classify_proxy_occupant, ensure_resolution_matches, ProxyResolution},
    transaction::{self, TransactionRecord},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    env, fs,
    path::{Component, Path, PathBuf},
    time::Duration,
};

const MARKER_FILE: &str = ".moddin-optiscaler.json";
const OPTISCALER_MAIN_DLL: &str = "OptiScaler.dll";
const OFFICIAL_RELEASE_PREFIX: &str = "https://github.com/optiscaler/OptiScaler/releases/download/";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OptiScalerRequest {
    pub game_id: String,
    pub game_name: String,
    pub install_dir: String,
    pub executable: String,
    pub version: String,
    pub download_url: String,
    pub sha256: String,
    pub proxy_candidates: Vec<String>,
    #[serde(default)]
    pub safety_notes: Vec<String>,
    /// Allows replacing a proxy DLL held by an unknown third-party loader
    /// (transactional backup still applies). Defaults to `false` so
    /// third-party DLLs are never overwritten implicitly.
    #[serde(default)]
    pub allow_replace_unknown: bool,
    /// Optional resolution pinned by the caller from a previous preview.
    /// Install recomputes the resolution and refuses mismatches, so a stale
    /// preview can never be replayed against a changed game directory.
    #[serde(default)]
    pub resolution: Option<ProxyResolution>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyConflict {
    pub name: String,
    pub path: String,
    pub size_bytes: u64,
    pub managed_by_moddin: bool,
    pub held_by: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OptiScalerPreview {
    pub can_apply: bool,
    pub game_running: bool,
    pub executable_exists: bool,
    pub executable_path: String,
    pub executable_directory: String,
    pub selected_proxy: Option<String>,
    pub resolution: ProxyResolution,
    pub installed: bool,
    pub installed_version: Option<String>,
    pub current_proxy: Option<String>,
    pub manual_install_detected: bool,
    pub conflicts: Vec<ProxyConflict>,
    pub changes: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OptiScalerMarker {
    version: String,
    proxy_dll: String,
    source_sha256: String,
    installed_files: Vec<String>,
}

fn safe_join_relative(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let relative_path = Path::new(relative);
    if relative_path.as_os_str().is_empty() {
        return Err("Catalog executable path is empty.".to_owned());
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

fn validate_request(request: &OptiScalerRequest) -> Result<(), String> {
    if !request.download_url.starts_with(OFFICIAL_RELEASE_PREFIX) {
        return Err("OptiScaler downloads must come from the official optiscaler/OptiScaler GitHub releases.".to_owned());
    }

    let sha = request.sha256.trim();
    if sha.len() != 64 || !sha.chars().all(|character| character.is_ascii_hexdigit()) {
        return Err("OptiScaler recipe has an invalid SHA-256 checksum.".to_owned());
    }

    if request.proxy_candidates.is_empty() {
        return Err("OptiScaler recipe does not define a proxy DLL candidate.".to_owned());
    }

    for proxy in &request.proxy_candidates {
        let lower = proxy.to_ascii_lowercase();
        if !lower.ends_with(".dll")
            || proxy.contains('/')
            || proxy.contains('\\')
            || proxy.contains("..")
        {
            return Err(format!("Invalid OptiScaler proxy DLL name: {proxy}"));
        }
    }

    Ok(())
}

fn executable_context(request: &OptiScalerRequest) -> Result<(PathBuf, PathBuf, String), String> {
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

fn read_marker(executable_directory: &Path) -> Option<OptiScalerMarker> {
    let contents = fs::read_to_string(marker_path(executable_directory)).ok()?;
    serde_json::from_str(&contents).ok()
}

struct ProxyChoice {
    selected: Option<String>,
    conflicts: Vec<ProxyConflict>,
    resolution: ProxyResolution,
}

fn choose_proxy(
    executable_directory: &Path,
    candidates: &[String],
    marker: Option<&OptiScalerMarker>,
    allow_replace_unknown: bool,
) -> ProxyChoice {
    if let Some(marker) = marker {
        let current = executable_directory.join(&marker.proxy_dll);
        if current.is_file() {
            // Updating our own managed install: the previous proxy copy is
            // backed up by the install transaction and replaced in place,
            // so Undo restores it.
            return ProxyChoice {
                selected: Some(marker.proxy_dll.clone()),
                conflicts: collect_conflicts(
                    executable_directory,
                    candidates,
                    Some(&marker.proxy_dll),
                ),
                resolution: ProxyResolution::ReplaceWithBackup,
            };
        }
    }

    // First pass: a genuinely free candidate always wins and every occupied
    // name stays untouched.
    for candidate in candidates {
        if !executable_directory.join(candidate).is_file() {
            return ProxyChoice {
                selected: Some(candidate.clone()),
                conflicts: collect_conflicts(executable_directory, candidates, None),
                resolution: ProxyResolution::UseNextFree,
            };
        }
    }

    // Second pass: every candidate is occupied. Prefer replacing a
    // Moddin-managed occupant (recoverable via Undo); a third-party DLL is
    // only replaced when the caller explicitly opts in.
    let conflicts = collect_conflicts(executable_directory, candidates, None);
    if let Some(managed) = conflicts.iter().find(|conflict| conflict.managed_by_moddin) {
        return ProxyChoice {
            selected: Some(managed.name.clone()),
            conflicts,
            resolution: ProxyResolution::ReplaceWithBackup,
        };
    }
    if allow_replace_unknown {
        return ProxyChoice {
            selected: candidates.first().cloned(),
            conflicts,
            resolution: ProxyResolution::ReplaceWithBackup,
        };
    }

    ProxyChoice {
        selected: None,
        conflicts,
        resolution: ProxyResolution::Blocked,
    }
}

fn collect_conflicts(
    executable_directory: &Path,
    candidates: &[String],
    exclude: Option<&str>,
) -> Vec<ProxyConflict> {
    candidates
        .iter()
        .filter(|candidate| {
            exclude
                .map(|exclude| !candidate.eq_ignore_ascii_case(exclude))
                .unwrap_or(true)
        })
        .filter_map(|candidate| {
            if executable_directory.join(candidate).is_file() {
                Some(proxy_conflict(executable_directory, candidate))
            } else {
                None
            }
        })
        .collect()
}

fn proxy_conflict(executable_directory: &Path, name: &str) -> ProxyConflict {
    let path = executable_directory.join(name);
    let (managed_by_moddin, held_by) = classify_proxy_occupant(executable_directory, name)
        .map(|occupant| (occupant.managed_by_moddin, occupant.held_by))
        .unwrap_or((false, "another loader (not managed by Moddin)".to_owned()));
    ProxyConflict {
        name: name.to_owned(),
        path: path.to_string_lossy().into_owned(),
        size_bytes: path.metadata().map(|metadata| metadata.len()).unwrap_or(0),
        managed_by_moddin,
        held_by,
    }
}

fn preview_optiscaler_sync(request: OptiScalerRequest) -> Result<OptiScalerPreview, String> {
    validate_request(&request)?;
    let (executable_path, executable_directory, process_name) = executable_context(&request)?;
    let marker = read_marker(&executable_directory);
    let game_running = is_process_running(&process_name);
    let ini_exists = executable_directory.join("OptiScaler.ini").is_file();
    let manual_install_detected = marker.is_none() && ini_exists;
    let ProxyChoice {
        selected: selected_proxy,
        conflicts,
        resolution,
    } = choose_proxy(
        &executable_directory,
        &request.proxy_candidates,
        marker.as_ref(),
        request.allow_replace_unknown,
    );

    let installed = marker.as_ref().is_some_and(|value| {
        value.installed_files.iter().all(|relative| {
            safe_join_relative(&executable_directory, relative).is_ok_and(|path| path.is_file())
        })
    });
    let installed_version = marker.as_ref().map(|value| value.version.clone());
    let current_proxy = marker.as_ref().map(|value| value.proxy_dll.clone());

    let mut changes = vec![format!(
        "Download OptiScaler {} from the official GitHub release and verify SHA-256 {}.",
        request.version, request.sha256
    )];

    if let Some(proxy) = &selected_proxy {
        if installed {
            changes.push(format!(
                "Update the Moddin-managed OptiScaler installation using {proxy}."
            ));
        } else {
            changes.push(format!(
                "Install OptiScaler beside the game executable using {proxy}."
            ));
        }
        if resolution == ProxyResolution::ReplaceWithBackup {
            let held_by = conflicts
                .iter()
                .find(|conflict| conflict.name.eq_ignore_ascii_case(proxy))
                .map(|conflict| conflict.held_by.as_str())
                .unwrap_or("the existing proxy");
            changes.push(format!(
                "Back up the existing '{proxy}' (held by {held_by}) and install the new proxy over it."
            ));
        }
        changes
            .push("Back up every file that will be replaced before writing anything.".to_owned());
        changes.push("Record every created file so Undo can remove it safely.".to_owned());
    }

    let mut warnings = request.safety_notes.clone();
    if game_running {
        warnings.push(format!(
            "{process_name} is running. Close the game before installing."
        ));
    }
    if manual_install_detected {
        warnings.push(
            "OptiScaler.ini already exists but this installation is not managed by Moddin. Automatic overwrite is blocked to protect the existing setup."
                .to_owned(),
        );
    }
    match resolution {
        ProxyResolution::Blocked => {
            warnings.push(format!(
                "All supported proxy DLL names for this recipe are already occupied: {}.",
                request.proxy_candidates.join(", ")
            ));
            warnings.push(
                "Moddin will not replace third-party proxy DLLs by default; the recipe must explicitly opt in with allowReplaceUnknown."
                    .to_owned(),
            );
        }
        ProxyResolution::ReplaceWithBackup => {
            if let Some(proxy) = selected_proxy.as_deref() {
                if let Some(conflict) = conflicts
                    .iter()
                    .find(|conflict| conflict.name.eq_ignore_ascii_case(proxy))
                {
                    warnings.push(format!(
                        "Proxy '{proxy}' is occupied by {}; Moddin will back it up and install over it (Undo restores the previous DLL).",
                        conflict.held_by
                    ));
                    if !conflict.managed_by_moddin {
                        warnings.push(
                            "allowReplaceUnknown is enabled: replacing a third-party proxy DLL that Moddin does not manage; the other tool may stop working."
                                .to_owned(),
                        );
                    }
                }
            }
        }
        ProxyResolution::UseNextFree if !conflicts.is_empty() => {
            warnings.push(format!(
                "Some proxy names are already occupied. Moddin will use {} instead and leave the other DLLs untouched.",
                selected_proxy.as_deref().unwrap_or_default()
            ));
        }
        _ => {}
    }

    Ok(OptiScalerPreview {
        can_apply: executable_path.is_file()
            && !game_running
            && !manual_install_detected
            && selected_proxy.is_some()
            && resolution != ProxyResolution::Blocked,
        game_running,
        executable_exists: executable_path.is_file(),
        executable_path: executable_path.to_string_lossy().into_owned(),
        executable_directory: executable_directory.to_string_lossy().into_owned(),
        selected_proxy,
        resolution,
        installed,
        installed_version,
        current_proxy,
        manual_install_detected,
        conflicts,
        changes,
        warnings,
    })
}

#[tauri::command]
pub async fn preview_optiscaler(request: OptiScalerRequest) -> Result<OptiScalerPreview, String> {
    tauri::async_runtime::spawn_blocking(move || preview_optiscaler_sync(request))
        .await
        .map_err(|error| format!("OptiScaler preview task failed: {error}"))?
}

fn collect_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    fn visit(directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
        for entry in fs::read_dir(directory)
            .map_err(|error| format!("Could not read extracted OptiScaler files: {error}"))?
        {
            let entry =
                entry.map_err(|error| format!("Could not read extracted entry: {error}"))?;
            let path = entry.path();
            if path.is_dir() {
                visit(&path, files)?;
            } else if path.is_file() {
                files.push(path);
            }
        }
        Ok(())
    }

    let mut files = Vec::new();
    visit(root, &mut files)?;
    Ok(files)
}

fn find_payload_root(extract_root: &Path) -> Result<(PathBuf, PathBuf), String> {
    let mut candidates = collect_files(extract_root)?
        .into_iter()
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.eq_ignore_ascii_case(OPTISCALER_MAIN_DLL))
        })
        .collect::<Vec<_>>();

    candidates.sort_by_key(|path| path.components().count());
    let main_dll = candidates
        .into_iter()
        .next()
        .ok_or_else(|| format!("The verified archive does not contain {OPTISCALER_MAIN_DLL}."))?;
    let payload_root = main_dll
        .parent()
        .ok_or_else(|| "Could not resolve OptiScaler archive payload root.".to_owned())?
        .to_path_buf();

    Ok((payload_root, main_dll))
}

fn normalized_relative(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn install_from_archive(
    request: OptiScalerRequest,
    archive_bytes: Vec<u8>,
) -> Result<TransactionRecord, String> {
    let preview = preview_optiscaler_sync(request.clone())?;
    if !preview.can_apply {
        return Err(
            "OptiScaler installation is not safe to apply in the current game environment."
                .to_owned(),
        );
    }
    // The preview above was recomputed from this exact request, so its
    // resolution is authoritative; a caller-pinned resolution is only
    // accepted when it still matches the environment.
    ensure_resolution_matches(request.resolution, preview.resolution, "OptiScaler")?;

    let selected_proxy = preview
        .selected_proxy
        .ok_or_else(|| "No safe proxy DLL name is available.".to_owned())?;
    let (_, executable_directory, process_name) = executable_context(&request)?;

    let actual_hash = format!("{:x}", Sha256::digest(&archive_bytes));
    if !actual_hash.eq_ignore_ascii_case(request.sha256.trim()) {
        return Err(format!(
            "OptiScaler SHA-256 mismatch. Expected {}, received {}. Nothing was installed.",
            request.sha256, actual_hash
        ));
    }

    let staging_root = env::temp_dir()
        .join("Moddin")
        .join(format!("optiscaler-{}", uuid::Uuid::new_v4().simple()));
    let archive_path = staging_root.join("optiscaler.7z");
    let extract_root = staging_root.join("extracted");
    fs::create_dir_all(&extract_root)
        .map_err(|error| format!("Could not create OptiScaler staging directory: {error}"))?;
    fs::write(&archive_path, archive_bytes)
        .map_err(|error| format!("Could not stage OptiScaler archive: {error}"))?;

    let result = (|| -> Result<TransactionRecord, String> {
        sevenz_rust::decompress_file(&archive_path, &extract_root)
            .map_err(|error| format!("Could not extract verified OptiScaler archive: {error}"))?;

        let (payload_root, main_dll) = find_payload_root(&extract_root)?;
        let payload_files = collect_files(&payload_root)?;
        let old_marker = read_marker(&executable_directory);

        let mut copy_pairs: Vec<(PathBuf, PathBuf, String)> = Vec::new();
        let mut installed_relative = Vec::new();

        for source in payload_files {
            if source == main_dll {
                continue;
            }
            let relative = source
                .strip_prefix(&payload_root)
                .map_err(|error| format!("Could not resolve OptiScaler payload path: {error}"))?;
            let relative_string = normalized_relative(relative);
            let target = executable_directory.join(relative);
            copy_pairs.push((source, target, relative_string.clone()));
            installed_relative.push(relative_string);
        }

        let proxy_target = executable_directory.join(&selected_proxy);
        copy_pairs.push((
            main_dll.clone(),
            proxy_target.clone(),
            selected_proxy.clone(),
        ));
        installed_relative.push(selected_proxy.clone());

        let marker_target = marker_path(&executable_directory);
        installed_relative.push(MARKER_FILE.to_owned());
        installed_relative.sort();
        installed_relative.dedup();

        let new_set = installed_relative
            .iter()
            .map(|path| path.to_ascii_lowercase())
            .collect::<HashSet<_>>();
        let mut stale_targets = Vec::new();
        if let Some(old_marker) = &old_marker {
            for relative in &old_marker.installed_files {
                if !new_set.contains(&relative.to_ascii_lowercase()) {
                    let target = safe_join_relative(&executable_directory, relative)?;
                    stale_targets.push(target);
                }
            }
        }

        let mut targets = copy_pairs
            .iter()
            .map(|(_, target, _)| target.clone())
            .collect::<Vec<_>>();
        targets.extend(stale_targets.iter().cloned());
        targets.push(marker_target.clone());

        let mut metadata = BTreeMap::new();
        metadata.insert("processName".to_owned(), process_name);
        metadata.insert("version".to_owned(), request.version.clone());
        metadata.insert("proxyDll".to_owned(), selected_proxy.clone());
        metadata.insert("sourceSha256".to_owned(), actual_hash.clone());
        metadata.insert(
            "proxyResolution".to_owned(),
            match preview.resolution {
                ProxyResolution::UseNextFree => "use_next_free",
                ProxyResolution::ReplaceWithBackup => "replace_with_backup",
                ProxyResolution::Blocked => "blocked",
            }
            .to_owned(),
        );

        let transaction = transaction::begin_file_set_transaction(
            &executable_directory,
            &targets,
            "optiscaler",
            &format!(
                "Install OptiScaler {} for {}",
                request.version, request.game_name
            ),
            &request.game_id,
            metadata,
        )?;

        let apply_result = (|| -> Result<(), String> {
            for (source, target, _) in &copy_pairs {
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent).map_err(|error| {
                        format!("Could not create OptiScaler target directory: {error}")
                    })?;
                }
                fs::copy(source, target).map_err(|error| {
                    format!(
                        "Could not install '{}' to '{}': {error}",
                        source.display(),
                        target.display()
                    )
                })?;
            }

            for stale in &stale_targets {
                if stale.is_file() {
                    fs::remove_file(stale).map_err(|error| {
                        format!(
                            "Could not remove stale OptiScaler file '{}': {error}",
                            stale.display()
                        )
                    })?;
                }
            }

            let marker = OptiScalerMarker {
                version: request.version.clone(),
                proxy_dll: selected_proxy.clone(),
                source_sha256: actual_hash.clone(),
                installed_files: installed_relative.clone(),
            };
            let marker_json = serde_json::to_string_pretty(&marker)
                .map_err(|error| format!("Could not serialize OptiScaler marker: {error}"))?;
            fs::write(&marker_target, marker_json)
                .map_err(|error| format!("Could not write Moddin OptiScaler marker: {error}"))?;

            if !proxy_target.is_file() || !executable_directory.join("OptiScaler.ini").is_file() {
                return Err("OptiScaler files failed post-install validation.".to_owned());
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
    })();

    let _ = fs::remove_dir_all(&staging_root);
    result
}

#[tauri::command]
pub async fn install_optiscaler(request: OptiScalerRequest) -> Result<TransactionRecord, String> {
    validate_request(&request)?;
    let preview = preview_optiscaler_sync(request.clone())?;
    if !preview.can_apply {
        return Err(
            "OptiScaler preview reports that installation is currently blocked.".to_owned(),
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
        .map_err(|error| format!("Could not download OptiScaler from GitHub: {error}"))?
        .error_for_status()
        .map_err(|error| {
            format!("GitHub returned an error while downloading OptiScaler: {error}")
        })?;
    let bytes = response
        .bytes()
        .await
        .map_err(|error| format!("Could not read OptiScaler download: {error}"))?
        .to_vec();

    tauri::async_runtime::spawn_blocking(move || install_from_archive(request, bytes))
        .await
        .map_err(|error| format!("OptiScaler installer task failed: {error}"))?
}

#[tauri::command]
pub fn uninstall_optiscaler(request: OptiScalerRequest) -> Result<TransactionRecord, String> {
    validate_request(&request)?;
    let (_, executable_directory, process_name) = executable_context(&request)?;
    if is_process_running(&process_name) {
        return Err(format!(
            "Close {process_name} before uninstalling OptiScaler."
        ));
    }
    if !marker_path(&executable_directory).is_file() {
        return Err("No Moddin-managed OptiScaler installation was found.".to_owned());
    }

    let executable_directory_string = executable_directory.to_string_lossy().into_owned();
    let mut last_rollback = None;
    loop {
        if read_marker(&executable_directory).is_none() {
            break;
        }

        let record = transaction::list_transactions_sync()?
            .into_iter()
            .find(|record| {
                record.status == "applied"
                    && record.kind == "optiscaler"
                    && record.game_id == request.game_id
                    && record.target_path == executable_directory_string
            })
            .ok_or_else(|| {
                "The managed OptiScaler marker exists, but its rollback transaction could not be found."
                    .to_owned()
            })?;

        last_rollback = Some(transaction::rollback_transaction(record.id)?);
    }

    last_rollback
        .ok_or_else(|| "No active OptiScaler installation transaction was found.".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> OptiScalerRequest {
        OptiScalerRequest {
            game_id: "cyberpunk-2077".to_owned(),
            game_name: "Cyberpunk 2077".to_owned(),
            install_dir: r"C:\Games\Cyberpunk 2077".to_owned(),
            executable: r"bin\x64\Cyberpunk2077.exe".to_owned(),
            version: "0.9.4".to_owned(),
            download_url: format!("{OFFICIAL_RELEASE_PREFIX}v0.9.4/file.7z"),
            sha256: "575cb4df866116093df75af607e37fd70e10f5163e0f23fd5c804142e80ef0ad".to_owned(),
            proxy_candidates: vec!["dxgi.dll".to_owned(), "wininet.dll".to_owned()],
            safety_notes: Vec::new(),
            allow_replace_unknown: false,
            resolution: None,
        }
    }

    fn temp_install(tag: &str) -> PathBuf {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let root = env::temp_dir().join(format!("moddin-optiscaler-{tag}-{suffix}"));
        fs::create_dir_all(&root).expect("temporary game directory");
        root
    }

    fn request_for(root: &Path) -> OptiScalerRequest {
        let mut request = request();
        request.install_dir = root.to_string_lossy().into_owned();
        request.executable = "moddin-proxy-test.exe".to_owned();
        request.proxy_candidates = vec!["dxgi.dll".to_owned(), "winmm.dll".to_owned()];
        request
    }

    #[test]
    fn refuses_unofficial_download_hosts() {
        let mut request = request();
        request.download_url = "https://example.com/OptiScaler.7z".to_owned();
        assert!(validate_request(&request).is_err());
    }

    #[test]
    fn validates_official_recipe() {
        assert!(validate_request(&request()).is_ok());
    }

    #[test]
    fn refuses_proxy_path_traversal() {
        let mut request = request();
        request.proxy_candidates = vec![r"..\dxgi.dll".to_owned()];
        assert!(validate_request(&request).is_err());
    }

    #[test]
    fn request_deserializes_with_safe_resolution_defaults() {
        let json = serde_json::json!({
            "gameId": "g",
            "gameName": "G",
            "installDir": "C:/Games/G",
            "executable": "game.exe",
            "version": "0.9.4",
            "downloadUrl": format!("{OFFICIAL_RELEASE_PREFIX}v0.9.4/file.7z"),
            "sha256": "575cb4df866116093df75af607e37fd70e10f5163e0f23fd5c804142e80ef0ad",
            "proxyCandidates": ["dxgi.dll"],
        });
        let parsed: OptiScalerRequest = serde_json::from_value(json).expect("request");
        assert!(!parsed.allow_replace_unknown);
        assert_eq!(parsed.resolution, None);

        let with_resolution = serde_json::json!({
            "gameId": "g",
            "gameName": "G",
            "installDir": "C:/Games/G",
            "executable": "game.exe",
            "version": "0.9.4",
            "downloadUrl": format!("{OFFICIAL_RELEASE_PREFIX}v0.9.4/file.7z"),
            "sha256": "575cb4df866116093df75af607e37fd70e10f5163e0f23fd5c804142e80ef0ad",
            "proxyCandidates": ["dxgi.dll"],
            "allowReplaceUnknown": true,
            "resolution": "replace_with_backup",
        });
        let parsed: OptiScalerRequest = serde_json::from_value(with_resolution).expect("request");
        assert!(parsed.allow_replace_unknown);
        assert_eq!(parsed.resolution, Some(ProxyResolution::ReplaceWithBackup));
    }

    #[test]
    fn free_candidate_resolves_use_next_free() {
        let root = temp_install("free");
        fs::write(root.join("moddin-proxy-test.exe"), b"exe").expect("executable");

        let preview = preview_optiscaler_sync(request_for(&root)).expect("preview");
        assert_eq!(preview.resolution, ProxyResolution::UseNextFree);
        assert_eq!(preview.selected_proxy.as_deref(), Some("dxgi.dll"));
        assert!(preview.can_apply);
        assert!(preview.conflicts.is_empty());

        fs::remove_dir_all(root).expect("temporary game cleanup");
    }

    #[test]
    fn unknown_occupants_block_by_default() {
        let root = temp_install("blocked");
        fs::write(root.join("moddin-proxy-test.exe"), b"exe").expect("executable");
        fs::write(root.join("dxgi.dll"), b"third-party").expect("occupant");
        fs::write(root.join("winmm.dll"), b"third-party").expect("occupant");

        let preview = preview_optiscaler_sync(request_for(&root)).expect("preview");
        assert_eq!(preview.resolution, ProxyResolution::Blocked);
        assert_eq!(preview.selected_proxy, None);
        assert!(!preview.can_apply);
        assert_eq!(preview.conflicts.len(), 2);
        assert!(preview
            .conflicts
            .iter()
            .all(|conflict| !conflict.managed_by_moddin));
        assert!(preview
            .conflicts
            .iter()
            .all(|conflict| conflict.held_by.contains("not managed")));

        fs::remove_dir_all(root).expect("temporary game cleanup");
    }

    #[test]
    fn unknown_occupants_replace_when_allowed() {
        let root = temp_install("allowed");
        fs::write(root.join("moddin-proxy-test.exe"), b"exe").expect("executable");
        fs::write(root.join("dxgi.dll"), b"third-party").expect("occupant");
        fs::write(root.join("winmm.dll"), b"third-party").expect("occupant");

        let mut request = request_for(&root);
        request.allow_replace_unknown = true;
        let preview = preview_optiscaler_sync(request).expect("preview");
        assert_eq!(preview.resolution, ProxyResolution::ReplaceWithBackup);
        assert_eq!(preview.selected_proxy.as_deref(), Some("dxgi.dll"));
        assert!(preview.can_apply);
        assert!(preview
            .warnings
            .iter()
            .any(|warning| warning.contains("allowReplaceUnknown")));

        fs::remove_dir_all(root).expect("temporary game cleanup");
    }

    #[test]
    fn managed_marker_update_uses_replace_with_backup() {
        let root = temp_install("managed-update");
        fs::write(root.join("moddin-proxy-test.exe"), b"exe").expect("executable");
        fs::write(root.join("dxgi.dll"), b"third-party").expect("occupant");
        fs::write(root.join("winmm.dll"), b"optiscaler").expect("own proxy");
        fs::write(
            root.join(MARKER_FILE),
            r#"{"version":"0.9.4","proxyDll":"winmm.dll","sourceSha256":"abc","installedFiles":["winmm.dll","OptiScaler.dll"]}"#,
        )
        .expect("marker");

        let preview = preview_optiscaler_sync(request_for(&root)).expect("preview");
        assert_eq!(preview.resolution, ProxyResolution::ReplaceWithBackup);
        assert_eq!(preview.selected_proxy.as_deref(), Some("winmm.dll"));
        assert!(preview.can_apply);
        assert_eq!(preview.conflicts.len(), 1);
        assert_eq!(preview.conflicts[0].name, "dxgi.dll");

        fs::remove_dir_all(root).expect("temporary game cleanup");
    }

    #[test]
    fn replace_with_backup_then_rollback_restores_proxy() {
        let root = temp_install("rollback");
        let proxy = root.join("dxgi.dll");
        fs::write(&proxy, b"original-proxy").expect("occupant");

        let record = transaction::begin_file_set_transaction(
            &root,
            std::slice::from_ref(&proxy),
            "optiscaler",
            "Replace proxy test",
            "proxy-test-game",
            BTreeMap::new(),
        )
        .expect("transaction");

        fs::write(&proxy, b"moddin-proxy").expect("replace occupant");
        let record = transaction::restore_record(record).expect("rollback");
        assert_eq!(record.status, "rolled_back");
        assert_eq!(fs::read(&proxy).expect("restored proxy"), b"original-proxy");

        fs::remove_dir_all(root).expect("temporary game cleanup");
    }
}
