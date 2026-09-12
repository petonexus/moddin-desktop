use crate::{ofxr, transaction::{self, TransactionRecord}};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VrIniPatch {
    pub section: String,
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VrLaunchRequest {
    pub game_id: String,
    pub game_name: String,
    pub install_dir: String,
    pub executable: String,
    #[serde(default)]
    pub arguments: Vec<String>,
    #[serde(default)]
    pub required_files: Vec<String>,
    pub config_path: Option<String>,
    #[serde(default)]
    pub config_patches: Vec<VrIniPatch>,
    #[serde(default)]
    pub safety_notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VrSettingStatus {
    pub section: String,
    pub key: String,
    pub value: String,
    pub current_value: Option<String>,
    pub will_change: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VrLaunchPreview {
    pub can_launch: bool,
    pub game_running: bool,
    pub executable_exists: bool,
    pub executable_path: String,
    pub executable_directory: String,
    pub active_openxr_runtime: Option<String>,
    pub missing_files: Vec<String>,
    pub config_path: Option<String>,
    pub config_exists: bool,
    pub settings: Vec<VrSettingStatus>,
    pub changes: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VrLaunchResult {
    pub process_id: u32,
    pub transaction: Option<TransactionRecord>,
}

fn safe_join_relative(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let relative_path = Path::new(relative);
    if relative_path.as_os_str().is_empty() {
        return Err("Catalog executable/config path is empty.".to_owned());
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

fn validate_request(request: &VrLaunchRequest) -> Result<(), String> {
    if request.game_id.trim().is_empty() || request.game_name.trim().is_empty() {
        return Err("VR launch recipe must identify the game.".to_owned());
    }

    if request
        .arguments
        .iter()
        .any(|argument| argument.contains('\0'))
    {
        return Err("VR launch arguments contain an invalid null character.".to_owned());
    }

    for file in &request.required_files {
        safe_join_relative(Path::new("."), file)?;
    }

    if let Some(config_path) = &request.config_path {
        safe_join_relative(Path::new("."), config_path)?;
    }

    for patch in &request.config_patches {
        if patch.section.trim().is_empty()
            || patch.key.trim().is_empty()
            || patch.section.contains('\r')
            || patch.section.contains('\n')
            || patch.section.contains('[')
            || patch.section.contains(']')
            || patch.key.contains('\r')
            || patch.key.contains('\n')
            || patch.key.contains('=')
            || patch.value.contains('\r')
            || patch.value.contains('\n')
        {
            return Err("VR INI recipe contains an invalid section, key, or value.".to_owned());
        }
    }

    Ok(())
}

fn executable_context(request: &VrLaunchRequest) -> Result<(PathBuf, PathBuf, String), String> {
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

fn registry_openxr_runtime(key: &str) -> Option<String> {
    let output = Command::new("reg.exe")
        .args(["query", key, "/v", "ActiveRuntime"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if !line.to_ascii_lowercase().contains("activeruntime") {
            continue;
        }

        let mut parts = line.split_whitespace();
        let _ = parts.next();
        let _ = parts.next();
        let value = parts.collect::<Vec<_>>().join(" ");
        if !value.is_empty() {
            return Some(value);
        }
    }

    None
}

fn active_openxr_runtime() -> Option<String> {
    [
        r"HKCU\Software\Khronos\OpenXR\1",
        r"HKLM\SOFTWARE\Khronos\OpenXR\1",
        r"HKLM\SOFTWARE\WOW6432Node\Khronos\OpenXR\1",
    ]
    .iter()
    .find_map(|key| registry_openxr_runtime(key))
}

fn ini_value(contents: &str, section: &str, key: &str) -> Option<String> {
    let mut current_section = String::new();

    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            current_section = trimmed[1..trimmed.len() - 1].trim().to_owned();
            continue;
        }

        if current_section != section {
            continue;
        }

        let Some((candidate_key, candidate_value)) = trimmed.split_once('=') else {
            continue;
        };
        if candidate_key.trim() == key {
            return Some(candidate_value.trim().to_owned());
        }
    }

    None
}

fn patch_ini(contents: &str, patches: &[VrIniPatch]) -> (String, bool) {
    let mut lines = contents
        .split_inclusive('\n')
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if lines.is_empty() && !contents.is_empty() {
        lines.push(contents.to_owned());
    }

    let mut current_section = String::new();
    let mut changed = false;

    for line in &mut lines {
        let content = line.trim_end_matches(['\r', '\n']);
        let trimmed = content.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            current_section = trimmed[1..trimmed.len() - 1].trim().to_owned();
            continue;
        }

        let Some((candidate_key, _)) = trimmed.split_once('=') else {
            continue;
        };
        let Some(patch) = patches
            .iter()
            .find(|patch| patch.section == current_section && patch.key == candidate_key.trim())
        else {
            continue;
        };

        let current = trimmed
            .split_once('=')
            .map(|(_, value)| value.trim().to_owned());
        if current.as_deref() == Some(patch.value.as_str()) {
            continue;
        }

        let indent_len = content.len() - content.trim_start().len();
        let indent = &content[..indent_len];
        let line_ending = if line.ends_with("\r\n") {
            "\r\n"
        } else if line.ends_with('\n') {
            "\n"
        } else {
            ""
        };
        *line = format!("{indent}{}={}{line_ending}", patch.key, patch.value);
        changed = true;
    }

    (lines.concat(), changed)
}

fn read_setting_status(config_contents: Option<&str>, patch: &VrIniPatch) -> VrSettingStatus {
    let current_value =
        config_contents.and_then(|contents| ini_value(contents, &patch.section, &patch.key));
    let will_change = current_value.as_deref() != Some(patch.value.as_str());

    VrSettingStatus {
        section: patch.section.clone(),
        key: patch.key.clone(),
        value: patch.value.clone(),
        current_value,
        will_change,
    }
}

fn config_context(
    request: &VrLaunchRequest,
    install_root: &Path,
) -> Result<(Option<PathBuf>, Option<String>), String> {
    let Some(relative) = &request.config_path else {
        return Ok((None, None));
    };

    let path = safe_join_relative(install_root, relative)?;
    let contents = fs::read_to_string(&path).ok();
    Ok((Some(path), contents))
}

fn preview_inner(request: &VrLaunchRequest) -> Result<VrLaunchPreview, String> {
    validate_request(request)?;
    let install_root = PathBuf::from(&request.install_dir);
    let (executable_path, executable_directory, process_name) = executable_context(request)?;
    let (config_path, config_contents) = config_context(request, &install_root)?;

    let missing_files = request
        .required_files
        .iter()
        .filter_map(|relative| {
            let path = safe_join_relative(&install_root, relative).ok()?;
            (!path.is_file()).then(|| relative.clone())
        })
        .collect::<Vec<_>>();

    let settings = request
        .config_patches
        .iter()
        .map(|patch| read_setting_status(config_contents.as_deref(), patch))
        .collect::<Vec<_>>();
    let game_running = is_process_running(&process_name);
    let active_runtime = active_openxr_runtime();
    let config_exists = config_path.as_ref().is_some_and(|path| path.is_file());

    let mut changes = vec![
        "Use the active Windows OpenXR runtime and start the VR headset before the game."
            .to_owned(),
        format!(
            "Start {} with the VR recipe from its executable directory.",
            process_name
        ),
    ];
    if !request.arguments.is_empty() {
        changes.push(format!(
            "Launch arguments: {}.",
            request.arguments.join(" ")
        ));
    }
    if let Some(path) = &config_path {
        if config_exists && settings.iter().any(|setting| setting.will_change) {
            changes.push(format!(
                "Update only the declared VR settings in '{}' after creating a backup.",
                path.display()
            ));
        }
    }

    let mut warnings = request.safety_notes.clone();
    if game_running {
        warnings.push(format!(
            "{process_name} is already running. Close it before launching VR."
        ));
    }
    if !missing_files.is_empty() {
        warnings.push(format!(
            "Required VR files were not found: {}.",
            missing_files.join(", ")
        ));
    }
    if active_runtime.is_none() {
        warnings.push(
            "No active OpenXR runtime was found in Windows. Select SteamVR, Meta Quest Link, Virtual Desktop VDXR, or another runtime first."
                .to_owned(),
        );
    }
    if request.config_path.is_some() && !config_exists {
        warnings.push(
            "The optional VR configuration file was not found. The game can still start, but Moddin cannot apply its declared INI settings."
                .to_owned(),
        );
    }
    if settings
        .iter()
        .any(|setting| setting.current_value.is_none())
        && config_exists
    {
        warnings.push(
            "One or more declared settings are not present in this version of the INI; Moddin will leave unknown keys untouched."
                .to_owned(),
        );
    }

    Ok(VrLaunchPreview {
        can_launch: executable_path.is_file()
            && !game_running
            && missing_files.is_empty()
            && active_runtime.is_some(),
        game_running,
        executable_exists: executable_path.is_file(),
        executable_path: executable_path.to_string_lossy().into_owned(),
        executable_directory: executable_directory.to_string_lossy().into_owned(),
        active_openxr_runtime: active_runtime,
        missing_files,
        config_path: config_path.map(|path| path.to_string_lossy().into_owned()),
        config_exists,
        settings,
        changes,
        warnings,
    })
}

#[tauri::command]
pub fn preview_vr_launch(request: VrLaunchRequest) -> Result<VrLaunchPreview, String> {
    preview_inner(&request)
}

#[tauri::command]
pub async fn launch_vr_game(
    request: VrLaunchRequest,
    ofxr_request: Option<ofxr::OfxrRequest>,
) -> Result<VrLaunchResult, String> {
    let preview = preview_inner(&request)?;
    if !preview.can_launch {
        return Err("VR launch is blocked by the current preflight checks.".to_owned());
    }

    if let Some(ofxr_request) = ofxr_request {
        let result = ofxr::install_ofxr(ofxr_request).await?;
        if !result.armed {
            return Err("OFXR Bridge could not be confirmed as armed before VR launch.".to_owned());
        }
    }

    let install_root = PathBuf::from(&request.install_dir);
    let (executable_path, executable_directory, process_name) = executable_context(&request)?;
    let config_path = request
        .config_path
        .as_deref()
        .map(|relative| safe_join_relative(&install_root, relative))
        .transpose()?;
    let config_changed = config_path.as_ref().is_some_and(|path| {
        path.is_file()
            && request.config_patches.iter().any(|patch| {
                ini_value(
                    &fs::read_to_string(path).unwrap_or_default(),
                    &patch.section,
                    &patch.key,
                )
                .as_deref()
                    != Some(patch.value.as_str())
            })
    });

    let mut transaction = None;
    if config_changed {
        let path = config_path
            .as_ref()
            .ok_or_else(|| "VR configuration path could not be resolved.".to_owned())?;
        let mut metadata = BTreeMap::new();
        metadata.insert("processName".to_owned(), process_name.clone());
        transaction = Some(transaction::backup_file_with_metadata(
            path,
            "vr-launch",
            &format!("Configure VR launch profile for {}", request.game_name),
            &request.game_id,
            metadata,
        )?);

        let contents = fs::read_to_string(path).map_err(|error| {
            format!(
                "Could not read VR configuration '{}': {error}",
                path.display()
            )
        })?;
        let (patched, changed) = patch_ini(&contents, &request.config_patches);
        if !changed {
            transaction = None;
        } else if let Err(error) = fs::write(path, patched) {
            if let Some(record) = transaction.take() {
                let _ = transaction::restore_record(record);
            }
            return Err(format!(
                "Could not apply VR configuration '{}': {error}",
                path.display()
            ));
        } else {
            let verify_contents = match fs::read_to_string(path) {
                Ok(contents) => contents,
                Err(error) => {
                    if let Some(record) = transaction.take() {
                        let _ = transaction::restore_record(record);
                    }
                    return Err(format!(
                        "Could not verify VR configuration '{}': {error}",
                        path.display()
                    ));
                }
            };
            let invalid = request.config_patches.iter().any(|patch| {
                ini_value(&verify_contents, &patch.section, &patch.key).as_deref()
                    != Some(patch.value.as_str())
            });
            if invalid {
                if let Some(record) = transaction.take() {
                    let _ = transaction::restore_record(record);
                }
                return Err(format!(
                    "VR configuration '{}' failed post-write verification.",
                    path.display()
                ));
            }
        }
    }

    let mut command = Command::new(&executable_path);
    command
        .args(&request.arguments)
        .current_dir(&executable_directory)
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    let child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            if let Some(record) = transaction.take() {
                let _ = transaction::restore_record(record);
            }
            return Err(format!(
                "Could not start '{}': {error}",
                executable_path.display()
            ));
        }
    };

    Ok(VrLaunchResult {
        process_id: child.id(),
        transaction,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_paths_that_escape_game_root() {
        let root = Path::new(r"C:\Games\Example");
        assert!(safe_join_relative(root, r"..\other\settings.ini").is_err());
        assert!(safe_join_relative(root, r"C:\Windows\notepad.exe").is_err());
    }

    #[test]
    fn reads_and_updates_only_declared_ini_keys() {
        let input =
            "[VR]\r\nStereoMode=cinema\r\nGameResScale=1.0\r\n\r\n[Other]\r\nValue=keep\r\n";
        let patches = vec![VrIniPatch {
            section: "VR".to_owned(),
            key: "GameResScale".to_owned(),
            value: "0.75".to_owned(),
        }];

        let (output, changed) = patch_ini(input, &patches);
        assert!(changed);
        assert!(output.contains("GameResScale=0.75\r\n"));
        assert!(output.contains("StereoMode=cinema\r\n"));
        assert!(output.contains("Value=keep\r\n"));
    }

    #[test]
    fn ini_lookup_is_section_aware() {
        let input = "[VR]\nFpsTarget=60\n[Other]\nFpsTarget=30\n";
        assert_eq!(ini_value(input, "VR", "FpsTarget").as_deref(), Some("60"));
        assert_eq!(
            ini_value(input, "Other", "FpsTarget").as_deref(),
            Some("30")
        );
    }
}
