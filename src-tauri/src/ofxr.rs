use crate::transaction::{self, TransactionRecord};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    env, fs,
    io::Cursor,
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::Duration,
};
use windows_sys::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::{
        FindWindowW, GetWindowThreadProcessId, SendMessageW, WM_CLOSE, WM_COMMAND,
    },
};
use zip::ZipArchive;

const OFFICIAL_RELEASE_PREFIX: &str =
    "https://github.com/tig3rmast3r/OFXR-Bridge/releases/download/";
const TRAY_EXECUTABLE: &str = "OFXRBridgeTray.exe";
const LAYER_DLL: &str = "ofxr/XR_APILAYER_XRFrameBridge_diagnostic.dll";
const MARKER_FILE: &str = ".moddin-ofxr.json";
const TRAY_WINDOW_CLASS: &str = "OFXRBridgeTrayWindow";
const OPENXR_REGISTRY_KEY: &str = r"HKCU\SOFTWARE\Khronos\OpenXR\1\ApiLayers\Implicit";
const OFXR_LOCAL_DIRECTORY: &str = "OFXR Bridge";
const ARM_COMMAND: usize = 90;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfxrRequest {
    pub game_id: String,
    pub game_name: String,
    pub install_dir: String,
    pub executable: String,
    pub version: String,
    pub implementation_version: u32,
    pub download_url: String,
    pub sha256: String,
    pub backend: String,
    pub nvidia_preset: String,
    pub nvidia_input_scale: u32,
    #[serde(default)]
    pub nvidia_bidirectional: bool,
    #[serde(default)]
    pub force_reinstall: bool,
    #[serde(default)]
    pub safety_notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OfxrPreview {
    pub can_apply: bool,
    pub game_running: bool,
    pub executable_exists: bool,
    pub install_directory: String,
    pub tray_path: String,
    pub tray_installed: bool,
    pub tray_running: bool,
    pub installed: bool,
    pub installed_version: Option<String>,
    pub configured: bool,
    pub armed: bool,
    pub runtime_manifest: Option<String>,
    pub changes: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OfxrResult {
    pub installed: bool,
    pub started: bool,
    pub armed: bool,
    pub version: String,
    pub backend: String,
    pub transaction: Option<TransactionRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OfxrMarker {
    version: String,
    implementation_version: u32,
    source_sha256: String,
    installed_files: Vec<String>,
    tray_executable: String,
}

fn local_app_data() -> PathBuf {
    env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir)
}

fn install_directory() -> PathBuf {
    local_app_data()
        .join("Moddin")
        .join("tools")
        .join("ofxr-bridge")
}

fn settings_directory() -> PathBuf {
    local_app_data().join(OFXR_LOCAL_DIRECTORY)
}

fn settings_path() -> PathBuf {
    settings_directory().join("tray.ini")
}

fn runtime_directory(implementation_version: u32) -> PathBuf {
    settings_directory()
        .join("RuntimeLayer")
        .join(format!("v{implementation_version:03}"))
}

fn marker_path() -> PathBuf {
    install_directory().join(MARKER_FILE)
}

fn safe_join_relative(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let relative_path = Path::new(relative);
    if relative_path.as_os_str().is_empty() {
        return Err("OFXR archive contains an empty path.".to_owned());
    }

    for component in relative_path.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(format!(
                    "OFXR archive path escapes its install directory: {relative}"
                ));
            }
        }
    }

    Ok(root.join(relative_path))
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

fn validate_request(request: &OfxrRequest) -> Result<(), String> {
    if !request.download_url.starts_with(OFFICIAL_RELEASE_PREFIX)
        || !request.download_url.to_ascii_lowercase().ends_with(".zip")
    {
        return Err(
            "OFXR downloads must come from an official OFXR-Bridge GitHub release archive."
                .to_owned(),
        );
    }

    let sha = request.sha256.trim();
    if sha.len() != 64 || !sha.chars().all(|character| character.is_ascii_hexdigit()) {
        return Err("OFXR recipe has an invalid SHA-256 checksum.".to_owned());
    }

    if request.version.trim().is_empty() || request.implementation_version == 0 {
        return Err("OFXR recipe is missing its release or implementation version.".to_owned());
    }
    if request.backend != "fidelityfx" && request.backend != "nvidia" {
        return Err("OFXR backend must be fidelityfx or nvidia.".to_owned());
    }
    if !matches!(request.nvidia_preset.as_str(), "fast" | "medium" | "slow") {
        return Err("OFXR NVIDIA preset must be fast, medium or slow.".to_owned());
    }
    if !matches!(request.nvidia_input_scale, 50 | 75 | 100) {
        return Err("OFXR NVIDIA input scale must be 50, 75 or 100.".to_owned());
    }
    Ok(())
}

fn executable_context(request: &OfxrRequest) -> Result<(PathBuf, String), String> {
    let install_root = PathBuf::from(&request.install_dir);
    if !install_root.is_dir() {
        return Err(format!(
            "Game install directory does not exist: {}",
            request.install_dir
        ));
    }

    let executable_path = safe_join_relative(&install_root, &request.executable)?;
    let process_name = executable_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "Could not resolve the game executable name.".to_owned())?
        .to_owned();
    Ok((executable_path, process_name))
}

fn is_process_running(image_name: &str) -> bool {
    let output = Command::new("tasklist")
        .args([
            "/FI",
            &format!("IMAGENAME eq {image_name}"),
            "/FO",
            "CSV",
            "/NH",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();

    let Ok(output) = output else {
        return false;
    };

    String::from_utf8_lossy(&output.stdout)
        .to_ascii_lowercase()
        .contains(&image_name.to_ascii_lowercase())
}

fn read_marker() -> Option<OfxrMarker> {
    let contents = fs::read_to_string(marker_path()).ok()?;
    serde_json::from_str(&contents).ok()
}

fn marker_is_installed(marker: Option<&OfxrMarker>) -> bool {
    let Some(marker) = marker else {
        return false;
    };
    marker.installed_files.iter().all(|relative| {
        safe_join_relative(&install_directory(), relative).is_ok_and(|path| path.is_file())
    })
}

fn read_ini_value(contents: &str, section: &str, key: &str) -> Option<String> {
    let mut current_section = String::new();
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            current_section = trimmed[1..trimmed.len() - 1].trim().to_ascii_lowercase();
            continue;
        }
        if current_section != section.to_ascii_lowercase() {
            continue;
        }
        let Some((name, value)) = trimmed.split_once('=') else {
            continue;
        };
        if name.trim().eq_ignore_ascii_case(key) {
            return Some(value.trim().to_owned());
        }
    }
    None
}

fn expected_tray_ini(request: &OfxrRequest) -> String {
    format!(
        "[tray]\r\nbackend={}\r\nnvidia_preset={}\r\nnvidia_input_scale={}\r\nnvidia_bidirectional={}\r\ndiagnostics=0\r\noverlay_position=upper_right\r\n",
        request.backend,
        request.nvidia_preset,
        request.nvidia_input_scale,
        if request.nvidia_bidirectional { 1 } else { 0 },
    )
}

fn settings_match(request: &OfxrRequest) -> bool {
    let Ok(contents) = fs::read_to_string(settings_path()) else {
        return false;
    };
    [
        ("backend", request.backend.as_str()),
        ("nvidia_preset", request.nvidia_preset.as_str()),
        (
            "nvidia_input_scale",
            &request.nvidia_input_scale.to_string(),
        ),
        (
            "nvidia_bidirectional",
            if request.nvidia_bidirectional {
                "1"
            } else {
                "0"
            },
        ),
    ]
    .iter()
    .all(|(key, expected)| read_ini_value(&contents, "tray", key).as_deref() == Some(*expected))
}

fn owned_manifest(manifest: &Path, implementation_version: u32) -> bool {
    owned_manifest_in(manifest, &runtime_directory(implementation_version))
}

fn owned_manifest_in(manifest: &Path, directory: &Path) -> bool {
    let Ok(relative) = manifest.strip_prefix(directory) else {
        return false;
    };
    if relative.components().count() != 1 {
        return false;
    }
    let name = relative.to_string_lossy();
    name.starts_with("XR_APILAYER_XRFrameBridge_manual-") && name.ends_with(".json")
}

fn registry_value_exists(manifest: &Path) -> bool {
    Command::new("reg.exe")
        .args([
            "query",
            OPENXR_REGISTRY_KEY,
            "/v",
            &manifest.to_string_lossy(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn registry_remove(manifest: &Path) -> bool {
    Command::new("reg.exe")
        .args([
            "delete",
            OPENXR_REGISTRY_KEY,
            "/v",
            &manifest.to_string_lossy(),
            "/f",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn find_armed_manifest(implementation_version: u32) -> Option<PathBuf> {
    let directory = runtime_directory(implementation_version);
    let entries = fs::read_dir(directory).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file()
            && owned_manifest(&path, implementation_version)
            && registry_value_exists(&path)
        {
            return Some(path);
        }
    }
    None
}

fn window_handle() -> Option<HWND> {
    let class = windows_wide(TRAY_WINDOW_CLASS);
    let handle = unsafe { FindWindowW(class.as_ptr(), std::ptr::null()) };
    (!handle.is_null()).then_some(handle)
}

fn windows_wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn tray_window_for_path(expected_path: &Path) -> Option<HWND> {
    let handle = window_handle()?;
    let mut process_id = 0u32;
    unsafe {
        GetWindowThreadProcessId(handle, &mut process_id);
    }
    if process_id == 0 {
        return None;
    }

    let output = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-Command",
            &format!(
                "(Get-Process -Id {process_id} -ErrorAction SilentlyContinue | Select-Object -First 1 -ExpandProperty Path)"
            ),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let actual = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if actual.is_empty() {
        return None;
    }
    let expected = fs::canonicalize(expected_path).unwrap_or_else(|_| expected_path.to_path_buf());
    let actual = fs::canonicalize(&actual).unwrap_or_else(|_| PathBuf::from(actual));
    expected
        .to_string_lossy()
        .eq_ignore_ascii_case(&actual.to_string_lossy())
        .then_some(handle)
}

fn wait_for_tray(tray_path: &Path) -> Result<(HWND, bool), String> {
    for _ in 0..100 {
        if let Some(handle) = tray_window_for_path(tray_path) {
            return Ok((handle, false));
        }
        thread::sleep(Duration::from_millis(100));
    }
    Err(format!(
        "OFXR tray did not expose its notification-area window: {}",
        tray_path.display()
    ))
}

fn start_tray() -> Result<(HWND, bool), String> {
    let tray_path = install_directory().join(TRAY_EXECUTABLE);
    if !tray_path.is_file() {
        return Err(format!(
            "OFXR tray executable is missing: {}",
            tray_path.display()
        ));
    }

    if let Some(handle) = tray_window_for_path(&tray_path) {
        return Ok((handle, false));
    }

    let was_running = is_process_running(TRAY_EXECUTABLE);
    if !was_running {
        Command::new(&tray_path)
            .current_dir(install_directory())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| format!("Could not start OFXRBridgeTray.exe: {error}"))?;
    }

    let (handle, _) = wait_for_tray(&tray_path)?;
    Ok((handle, !was_running))
}

fn close_tray() -> Result<(), String> {
    let tray_path = install_directory().join(TRAY_EXECUTABLE);
    let Some(handle) = tray_window_for_path(&tray_path) else {
        if is_process_running(TRAY_EXECUTABLE) {
            return Err(
                "OFXRBridgeTray.exe is running but its tray window could not be verified."
                    .to_owned(),
            );
        }
        return Ok(());
    };

    unsafe {
        SendMessageW(handle, WM_CLOSE, 0, 0);
    }
    for _ in 0..100 {
        if tray_window_for_path(&tray_path).is_none() && !is_process_running(TRAY_EXECUTABLE) {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
    Err("OFXRBridgeTray.exe did not close and could not be safely removed.".to_owned())
}

fn extract_archive(bytes: &[u8], extract_root: &Path) -> Result<(), String> {
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| format!("Could not read the verified OFXR ZIP archive: {error}"))?;

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("Could not read OFXR archive entry: {error}"))?;
        if entry.name().ends_with('/') {
            continue;
        }
        let relative = entry
            .enclosed_name()
            .ok_or_else(|| format!("OFXR archive contains an unsafe path: {}", entry.name()))?
            .to_path_buf();
        let target = extract_root.join(&relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Could not create OFXR staging directory: {error}"))?;
        }
        let mut output = fs::File::create(&target).map_err(|error| {
            format!(
                "Could not stage OFXR file '{}': {error}",
                relative.display()
            )
        })?;
        std::io::copy(&mut entry, &mut output).map_err(|error| {
            format!(
                "Could not extract OFXR file '{}': {error}",
                relative.display()
            )
        })?;
    }
    Ok(())
}

fn collect_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    fn visit(directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
        for entry in fs::read_dir(directory)
            .map_err(|error| format!("Could not read staged OFXR files: {error}"))?
        {
            let entry =
                entry.map_err(|error| format!("Could not read staged OFXR entry: {error}"))?;
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

fn install_from_archive(
    request: OfxrRequest,
    archive_bytes: Vec<u8>,
) -> Result<TransactionRecord, String> {
    let actual_hash = format!("{:x}", Sha256::digest(&archive_bytes));
    if !actual_hash.eq_ignore_ascii_case(request.sha256.trim()) {
        return Err(format!(
            "OFXR SHA-256 mismatch. Expected {}, received {}. Nothing was installed.",
            request.sha256, actual_hash
        ));
    }

    let target_root = install_directory();
    if target_root.exists()
        && fs::read_dir(&target_root)
            .map_err(|error| format!("Could not inspect the OFXR install directory: {error}"))?
            .next()
            .is_some()
        && read_marker().is_none()
    {
        return Err(format!(
            "The OFXR install directory is not managed by Moddin and will not be overwritten: {}",
            target_root.display()
        ));
    }

    let staging_root = env::temp_dir()
        .join("Moddin")
        .join(format!("ofxr-{}", uuid::Uuid::new_v4().simple()));
    let extract_root = staging_root.join("extracted");
    fs::create_dir_all(&extract_root)
        .map_err(|error| format!("Could not create OFXR staging directory: {error}"))?;

    let result = (|| -> Result<TransactionRecord, String> {
        extract_archive(&archive_bytes, &extract_root)?;
        let tray_source = extract_root.join(TRAY_EXECUTABLE);
        let layer_source = extract_root.join(LAYER_DLL.replace('/', std::path::MAIN_SEPARATOR_STR));
        if !tray_source.is_file() || !layer_source.is_file() {
            return Err(
                "The verified OFXR archive is incomplete (tray or bridge layer missing)."
                    .to_owned(),
            );
        }

        let old_marker = read_marker();
        let payload_files = collect_files(&extract_root)?;
        let mut copy_pairs = Vec::new();
        let mut installed_relative = Vec::new();
        for source in payload_files {
            let relative = source
                .strip_prefix(&extract_root)
                .map_err(|error| format!("Could not resolve staged OFXR path: {error}"))?;
            let relative_string = normalized_relative(relative);
            let target = safe_join_relative(&target_root, &relative_string)?;
            copy_pairs.push((source, target));
            installed_relative.push(relative_string);
        }

        installed_relative.push(MARKER_FILE.to_owned());
        installed_relative.sort();
        installed_relative.dedup();

        let new_files = installed_relative
            .iter()
            .map(|path| path.to_ascii_lowercase())
            .collect::<std::collections::HashSet<_>>();
        let mut stale_targets = Vec::new();
        if let Some(marker) = &old_marker {
            for relative in &marker.installed_files {
                if !new_files.contains(&relative.to_ascii_lowercase()) {
                    stale_targets.push(safe_join_relative(&target_root, relative)?);
                }
            }
        }

        let marker_target = marker_path();
        let config_target = settings_path();
        let mut targets = copy_pairs
            .iter()
            .map(|(_, target)| target.clone())
            .collect::<Vec<_>>();
        targets.extend(stale_targets.iter().cloned());
        targets.push(marker_target.clone());
        targets.push(config_target.clone());

        let mut metadata = BTreeMap::new();
        metadata.insert("processName".to_owned(), TRAY_EXECUTABLE.to_owned());
        metadata.insert("version".to_owned(), request.version.clone());
        metadata.insert(
            "implementationVersion".to_owned(),
            request.implementation_version.to_string(),
        );
        metadata.insert("sourceSha256".to_owned(), actual_hash.clone());

        let transaction = transaction::begin_file_set_transaction(
            &local_app_data(),
            &targets,
            "ofxr-framegen",
            &format!(
                "Install OFXR Bridge {} for {}",
                request.version, request.game_name
            ),
            &request.game_id,
            metadata,
        )?;

        let apply_result = (|| -> Result<(), String> {
            for (source, target) in &copy_pairs {
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent).map_err(|error| {
                        format!("Could not create OFXR target directory: {error}")
                    })?;
                }
                fs::copy(source, target).map_err(|error| {
                    format!(
                        "Could not install OFXR file '{}': {error}",
                        target.display()
                    )
                })?;
            }
            for stale in &stale_targets {
                if stale.is_file() {
                    fs::remove_file(stale).map_err(|error| {
                        format!(
                            "Could not remove stale OFXR file '{}': {error}",
                            stale.display()
                        )
                    })?;
                }
            }

            let marker = OfxrMarker {
                version: request.version.clone(),
                implementation_version: request.implementation_version,
                source_sha256: actual_hash.clone(),
                installed_files: installed_relative.clone(),
                tray_executable: TRAY_EXECUTABLE.to_owned(),
            };
            let marker_json = serde_json::to_string_pretty(&marker)
                .map_err(|error| format!("Could not serialize OFXR marker: {error}"))?;
            fs::write(&marker_target, marker_json)
                .map_err(|error| format!("Could not write Moddin OFXR marker: {error}"))?;
            fs::create_dir_all(config_target.parent().unwrap_or_else(|| Path::new("."))).map_err(
                |error| format!("Could not create OFXR configuration directory: {error}"),
            )?;
            fs::write(&config_target, expected_tray_ini(&request))
                .map_err(|error| format!("Could not write OFXR tray configuration: {error}"))?;

            if !target_root.join(TRAY_EXECUTABLE).is_file()
                || !target_root
                    .join(LAYER_DLL.replace('/', std::path::MAIN_SEPARATOR_STR))
                    .is_file()
            {
                return Err("OFXR files failed post-install validation.".to_owned());
            }
            Ok(())
        })();

        if let Err(error) = apply_result {
            return match transaction::restore_record(transaction) {
                Ok(_) => Err(format!(
                    "{error} All changed OFXR files were restored automatically."
                )),
                Err(restore_error) => Err(format!(
                    "{error} Automatic OFXR rollback also failed: {restore_error}"
                )),
            };
        }

        transaction::mark_applied(transaction)
    })();
    let _ = fs::remove_dir_all(&staging_root);
    result
}

fn write_settings_transaction(request: &OfxrRequest) -> Result<Option<TransactionRecord>, String> {
    let target = settings_path();
    let expected = expected_tray_ini(request);
    if fs::read_to_string(&target).ok().as_deref() == Some(expected.as_str()) {
        return Ok(None);
    }

    let mut metadata = BTreeMap::new();
    metadata.insert("processName".to_owned(), TRAY_EXECUTABLE.to_owned());
    metadata.insert("version".to_owned(), request.version.clone());
    let transaction = transaction::begin_file_set_transaction(
        &local_app_data(),
        std::slice::from_ref(&target),
        "ofxr-framegen",
        &format!("Configure OFXR Bridge for {}", request.game_name),
        &request.game_id,
        metadata,
    )?;

    let apply_result = (|| -> Result<(), String> {
        fs::create_dir_all(target.parent().unwrap_or_else(|| Path::new(".")))
            .map_err(|error| format!("Could not create OFXR configuration directory: {error}"))?;
        fs::write(&target, expected)
            .map_err(|error| format!("Could not write OFXR tray configuration: {error}"))?;
        if fs::read_to_string(&target).ok().as_deref() != Some(expected_tray_ini(request).as_str())
        {
            return Err("OFXR tray configuration failed post-write verification.".to_owned());
        }
        Ok(())
    })();

    if let Err(error) = apply_result {
        return match transaction::restore_record(transaction) {
            Ok(_) => Err(format!(
                "{error} OFXR configuration was restored automatically."
            )),
            Err(restore_error) => Err(format!(
                "{error} Automatic OFXR configuration rollback also failed: {restore_error}"
            )),
        };
    }
    transaction::mark_applied(transaction).map(Some)
}

fn preview_inner(request: &OfxrRequest) -> Result<OfxrPreview, String> {
    validate_request(request)?;
    let (executable_path, process_name) = executable_context(request)?;
    let marker = read_marker();
    let installed = marker_is_installed(marker.as_ref());
    let tray_path = install_directory().join(TRAY_EXECUTABLE);
    let tray_installed = tray_path.is_file();
    let tray_running = tray_window_for_path(&tray_path).is_some();
    let game_running = is_process_running(&process_name);
    let configured = settings_match(request);
    let runtime_manifest = find_armed_manifest(request.implementation_version);
    let armed = runtime_manifest.is_some();

    let mut changes = Vec::new();
    if !installed
        || marker.as_ref().is_some_and(|value| {
            value.version != request.version
                || !value
                    .source_sha256
                    .eq_ignore_ascii_case(request.sha256.trim())
        })
    {
        changes.push(format!(
            "Download OFXR Bridge {} from the official GitHub release and verify SHA-256 {}.",
            request.version, request.sha256
        ));
        changes.push(
            "Install the complete tray and OpenXR layer archive in Moddin's user tools directory."
                .to_owned(),
        );
    }
    if !configured {
        changes.push(format!(
            "Configure {} with {} NVIDIA OFA scale {}%.",
            request.backend, request.nvidia_preset, request.nvidia_input_scale
        ));
    }
    if !tray_running {
        changes.push("Start OFXRBridgeTray.exe in the Windows notification area.".to_owned());
    }
    if !armed {
        changes.push(
            "Arm OFXR Bridge until manual disarm and verify its OpenXR registration.".to_owned(),
        );
    }

    let mut warnings = request.safety_notes.clone();
    if game_running {
        warnings.push(format!(
            "{process_name} is running. Close the game before configuring or arming OFXR."
        ));
    }
    if !executable_path.is_file() {
        warnings.push("The catalogued game executable was not found.".to_owned());
    }

    Ok(OfxrPreview {
        can_apply: executable_path.is_file() && !game_running,
        game_running,
        executable_exists: executable_path.is_file(),
        install_directory: install_directory().to_string_lossy().into_owned(),
        tray_path: tray_path.to_string_lossy().into_owned(),
        tray_installed,
        tray_running,
        installed,
        installed_version: marker.map(|value| value.version),
        configured,
        armed,
        runtime_manifest: runtime_manifest.map(|path| path.to_string_lossy().into_owned()),
        changes,
        warnings,
    })
}

fn rollback_after_activation_failure(
    transaction: Option<TransactionRecord>,
    error: String,
) -> String {
    let Some(transaction) = transaction else {
        return error;
    };
    let _ = close_tray();
    match transaction::restore_record(transaction) {
        Ok(_) => format!("{error} OFXR was disarmed and the changed files/configuration were rolled back automatically."),
        Err(restore_error) => format!("{error} OFXR activation failed and automatic rollback also failed: {restore_error}"),
    }
}

fn recover_previous_tray(request: &OfxrRequest, marker: Option<&OfxrMarker>) {
    let Some(marker) = marker else {
        return;
    };
    let mut recovery_request = request.clone();
    recovery_request.version = marker.version.clone();
    recovery_request.implementation_version = marker.implementation_version;
    recovery_request.sha256 = marker.source_sha256.clone();
    recovery_request.force_reinstall = false;
    let _ = activate(&recovery_request, None);
}

fn activate(
    request: &OfxrRequest,
    transaction: Option<TransactionRecord>,
) -> Result<OfxrResult, String> {
    if find_armed_manifest(request.implementation_version).is_none() {
        remove_stale_manifests();
    }
    let (handle, started) = match start_tray() {
        Ok(value) => value,
        Err(error) => return Err(rollback_after_activation_failure(transaction, error)),
    };

    let mut armed = find_armed_manifest(request.implementation_version).is_some();
    if !armed {
        unsafe {
            SendMessageW(handle, WM_COMMAND, ARM_COMMAND, 0);
        }
        for _ in 0..100 {
            if find_armed_manifest(request.implementation_version).is_some() {
                armed = true;
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
    }

    if !armed {
        return Err(rollback_after_activation_failure(
            transaction,
            "OFXR tray started, but the OpenXR bridge could not be confirmed as armed.".to_owned(),
        ));
    }

    Ok(OfxrResult {
        installed: marker_is_installed(read_marker().as_ref()),
        started,
        armed,
        version: request.version.clone(),
        backend: request.backend.clone(),
        transaction,
    })
}

#[tauri::command]
pub fn preview_ofxr(request: OfxrRequest) -> Result<OfxrPreview, String> {
    preview_inner(&request)
}

#[tauri::command]
pub async fn install_ofxr(request: OfxrRequest) -> Result<OfxrResult, String> {
    validate_request(&request)?;
    let preview = preview_inner(&request)?;
    if !preview.can_apply {
        return Err("OFXR cannot be configured while the game is running or missing.".to_owned());
    }

    let needs_install = request.force_reinstall
        || !preview.installed
        || preview.installed_version.as_deref() != Some(request.version.as_str());
    let previous_marker = read_marker();
    let mut transaction = None;

    if needs_install {
        let client = reqwest::Client::builder()
            .user_agent("Moddin/0.1 (+https://github.com/marcoasjunior/moddin)")
            .connect_timeout(Duration::from_secs(30))
            .timeout(Duration::from_secs(300))
            .build()
            .map_err(|error| format!("Could not create secure OFXR download client: {error}"))?;
        let response = client
            .get(&request.download_url)
            .send()
            .await
            .map_err(|error| format!("Could not download OFXR Bridge from GitHub: {error}"))?
            .error_for_status()
            .map_err(|error| {
                format!("GitHub returned an error while downloading OFXR Bridge: {error}")
            })?;
        let bytes = response
            .bytes()
            .await
            .map_err(|error| format!("Could not read OFXR Bridge download: {error}"))?
            .to_vec();
        let actual_hash = format!("{:x}", Sha256::digest(&bytes));
        if !actual_hash.eq_ignore_ascii_case(request.sha256.trim()) {
            return Err(format!(
                "OFXR SHA-256 mismatch. Expected {}, received {}. Nothing was installed.",
                request.sha256, actual_hash
            ));
        }
        if preview.tray_running || is_process_running(TRAY_EXECUTABLE) {
            close_tray()?;
        }
        let request_for_install = request.clone();
        let install_result = tauri::async_runtime::spawn_blocking(move || {
            install_from_archive(request_for_install, bytes)
        })
        .await
        .map_err(|error| format!("OFXR installer task failed: {error}"))?;
        match install_result {
            Ok(record) => transaction = Some(record),
            Err(error) => {
                recover_previous_tray(&request, previous_marker.as_ref());
                return Err(error);
            }
        }
    } else if !preview.configured {
        if preview.tray_running || is_process_running(TRAY_EXECUTABLE) {
            close_tray()?;
        }
        match write_settings_transaction(&request) {
            Ok(record) => transaction = record,
            Err(error) => {
                recover_previous_tray(&request, previous_marker.as_ref());
                return Err(error);
            }
        }
    }

    let result = activate(&request, transaction);
    if result.is_err() {
        recover_previous_tray(&request, previous_marker.as_ref());
    }
    result
}

fn remove_stale_manifests() {
    let root = settings_directory().join("RuntimeLayer");
    let Ok(directories) = fs::read_dir(root) else {
        return;
    };
    for directory in directories.flatten().filter(|entry| entry.path().is_dir()) {
        let directory_path = directory.path();
        let Ok(entries) = fs::read_dir(&directory_path) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && owned_manifest_in(&path, &directory_path) {
                if registry_value_exists(&path) {
                    let _ = registry_remove(&path);
                }
                let _ = fs::remove_file(path);
            }
        }
    }
}

#[tauri::command]
pub fn uninstall_ofxr(request: OfxrRequest) -> Result<TransactionRecord, String> {
    validate_request(&request)?;
    let (_, process_name) = executable_context(&request)?;
    if is_process_running(&process_name) {
        return Err(format!(
            "Close {process_name} before uninstalling OFXR Bridge."
        ));
    }

    close_tray()?;
    remove_stale_manifests();

    let mut last_transaction = None;
    for _ in 0..16 {
        if read_marker().is_none() {
            break;
        }
        let record = transaction::list_transactions()?
            .into_iter()
            .find(|record| {
                record.status == "applied"
                    && record.kind == "ofxr-framegen"
                    && record.game_id == request.game_id
            })
            .ok_or_else(|| {
                "OFXR is marked as installed, but no Moddin transaction can safely remove it."
                    .to_owned()
            })?;
        last_transaction = Some(transaction::rollback_transaction(record.id)?);
    }

    last_transaction
        .ok_or_else(|| "No Moddin-managed OFXR Bridge installation was found.".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> OfxrRequest {
        OfxrRequest {
            game_id: "example".to_owned(),
            game_name: "Example".to_owned(),
            install_dir: r"C:\Games\Example".to_owned(),
            executable: "game.exe".to_owned(),
            version: "0.2.0".to_owned(),
            implementation_version: 68,
            download_url: "https://github.com/tig3rmast3r/OFXR-Bridge/releases/download/0.2.0/OFXR-Bridge-v0.2.0-V068.zip".to_owned(),
            sha256: "3a4db67abd3d7fd013c9ef6878d8b7e23d312e84fe9c2c39ae509e4e554cb7f4".to_owned(),
            backend: "fidelityfx".to_owned(),
            nvidia_preset: "medium".to_owned(),
            nvidia_input_scale: 50,
            nvidia_bidirectional: false,
            force_reinstall: false,
            safety_notes: Vec::new(),
        }
    }

    #[test]
    fn refuses_unofficial_download_hosts() {
        let mut value = request();
        value.download_url = "https://example.com/OFXR-Bridge.zip".to_owned();
        assert!(validate_request(&value).is_err());
    }

    #[test]
    fn refuses_archive_path_traversal() {
        assert!(safe_join_relative(Path::new(r"C:\Tools\OFXR"), r"..\escape.dll").is_err());
        assert!(
            safe_join_relative(Path::new(r"C:\Tools\OFXR"), r"C:\Windows\system32.dll").is_err()
        );
    }

    #[test]
    fn writes_the_documented_tray_defaults() {
        let value = expected_tray_ini(&request());
        assert!(value.contains("backend=fidelityfx\r\n"));
        assert!(value.contains("nvidia_input_scale=50\r\n"));
        assert!(value.contains("nvidia_bidirectional=0\r\n"));
    }
}
