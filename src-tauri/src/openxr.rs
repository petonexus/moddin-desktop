use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

const OPENXR_REGISTRY_KEY: &str = r"HKLM\SOFTWARE\Khronos\OpenXR\1";
const OPENXR_AVAILABLE_REGISTRY_KEY: &str =
    r"HKLM\SOFTWARE\Khronos\OpenXR\1\AvailableRuntimes";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenXrRuntimeInfo {
    pub name: String,
    pub manifest_path: String,
    pub library_path: Option<String>,
    pub manifest_exists: bool,
    pub library_exists: bool,
    pub enabled: bool,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenXrState {
    pub active_runtime: Option<String>,
    pub active_runtime_name: Option<String>,
    pub game_override: Option<String>,
    pub game_override_name: Option<String>,
    pub effective_runtime: Option<String>,
    pub effective_runtime_name: Option<String>,
    pub effective_source: String,
    pub runtimes: Vec<OpenXrRuntimeInfo>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpenXrGamePreference {
    manifest_path: String,
}

fn valid_game_id(game_id: &str) -> bool {
    !game_id.is_empty()
        && game_id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
}

fn preferences_root() -> PathBuf {
    env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir)
        .join("Moddin")
        .join("profiles")
        .join("openxr")
}

fn preference_path(game_id: &str) -> Result<PathBuf, String> {
    if !valid_game_id(game_id) {
        return Err("Invalid game id for OpenXR preference storage.".to_owned());
    }
    Ok(preferences_root().join(format!("{game_id}.json")))
}

fn parse_active_runtime_output(output: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let trimmed = line.trim();
        let (_, value) = trimmed.split_once("REG_SZ")?;
        let path = value.trim();
        (!path.is_empty()).then(|| path.to_owned())
    })
}

fn parse_available_runtime_output(output: &str) -> Vec<(String, bool)> {
    output
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            let (name, raw_value) = trimmed.split_once("REG_DWORD")?;
            let path = name.trim();
            if path.is_empty() {
                return None;
            }
            let raw_value = raw_value.trim().split_whitespace().next().unwrap_or_default();
            let enabled = matches!(raw_value, "0x0" | "0");
            Some((path.to_owned(), enabled))
        })
        .collect()
}

pub(crate) fn system_active_runtime() -> Option<String> {
    let output = Command::new("reg.exe")
        .args(["query", OPENXR_REGISTRY_KEY, "/v", "ActiveRuntime"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    parse_active_runtime_output(&String::from_utf8_lossy(&output.stdout))
}

fn available_registry_runtimes() -> Vec<(String, bool)> {
    let Ok(output) = Command::new("reg.exe")
        .args(["query", OPENXR_AVAILABLE_REGISTRY_KEY])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
    else {
        return Vec::new();
    };

    if !output.status.success() {
        return Vec::new();
    }

    parse_available_runtime_output(&String::from_utf8_lossy(&output.stdout))
}

fn known_runtime_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Some(program_files) = env::var_os("ProgramFiles") {
        let root = PathBuf::from(program_files);
        candidates.push(root.join(r"Oculus\Support\oculus-runtime\oculus_openxr_64.json"));
        candidates.push(root.join(r"Virtual Desktop Streamer\OpenXR\virtualdesktop-openxr.json"));
        candidates.push(root.join(r"Varjo\varjo-openxr\VarjoOpenXR.json"));
    }

    if let Some(program_files_x86) = env::var_os("ProgramFiles(x86)") {
        let root = PathBuf::from(program_files_x86);
        candidates.push(root.join(r"Steam\steamapps\common\SteamVR\steamxr_win64.json"));
        candidates.push(root.join(r"VIVE\Updater\App\ViveVRRuntime\ViveVR_openxr\ViveOpenXR.json"));
    }

    if let Some(system_root) = env::var_os("SystemRoot") {
        candidates.push(PathBuf::from(system_root).join("System32").join("MixedRealityRuntime.json"));
    }

    candidates
}

fn runtime_name_fallback(path: &Path) -> String {
    let lower = path.to_string_lossy().to_ascii_lowercase();
    if lower.contains("steamvr") || lower.contains("steamxr") {
        return "SteamVR".to_owned();
    }
    if lower.contains("oculus") || lower.contains("meta") {
        return "Meta Quest Link".to_owned();
    }
    if lower.contains("virtual desktop") || lower.contains("virtualdesktop") {
        return "Virtual Desktop VDXR".to_owned();
    }
    if lower.contains("mixedreality") {
        return "Windows Mixed Reality".to_owned();
    }
    if lower.contains("varjo") {
        return "Varjo OpenXR".to_owned();
    }
    if lower.contains("vive") {
        return "VIVE OpenXR".to_owned();
    }

    path.file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("OpenXR runtime")
        .to_owned()
}

fn runtime_manifest(path: &Path, enabled: bool, active: bool) -> OpenXrRuntimeInfo {
    let manifest_exists = path.is_file();
    let manifest_json = fs::read_to_string(path)
        .ok()
        .and_then(|contents| serde_json::from_str::<Value>(&contents).ok());

    let name = manifest_json
        .as_ref()
        .and_then(|json| json.get("name"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| runtime_name_fallback(path));

    let library_path = manifest_json
        .as_ref()
        .and_then(|json| json.pointer("/runtime/library_path"))
        .and_then(Value::as_str)
        .map(str::to_owned);

    let library_exists = library_path
        .as_deref()
        .map(PathBuf::from)
        .map(|candidate| {
            if candidate.is_absolute() {
                candidate.is_file()
            } else {
                path.parent()
                    .map(|parent| parent.join(candidate).is_file())
                    .unwrap_or(false)
            }
        })
        .unwrap_or(false);

    OpenXrRuntimeInfo {
        name,
        manifest_path: path.to_string_lossy().into_owned(),
        library_path,
        manifest_exists,
        library_exists,
        enabled,
        active,
    }
}

fn normalize_path(path: &str) -> String {
    path.replace('/', "\\").to_ascii_lowercase()
}

fn discover_runtimes(active_runtime: Option<&str>) -> Vec<OpenXrRuntimeInfo> {
    let mut candidates = BTreeMap::<String, (String, bool)>::new();

    for (path, enabled) in available_registry_runtimes() {
        candidates.insert(normalize_path(&path), (path, enabled));
    }

    for path in known_runtime_candidates() {
        if path.is_file() {
            let string = path.to_string_lossy().into_owned();
            candidates
                .entry(normalize_path(&string))
                .or_insert((string, true));
        }
    }

    if let Some(active) = active_runtime {
        candidates
            .entry(normalize_path(active))
            .or_insert((active.to_owned(), true));
    }

    let active_normalized = active_runtime.map(normalize_path);
    let mut runtimes = candidates
        .into_values()
        .map(|(path, enabled)| {
            let active = active_normalized
                .as_deref()
                .is_some_and(|current| current == normalize_path(&path));
            runtime_manifest(Path::new(&path), enabled, active)
        })
        .collect::<Vec<_>>();

    runtimes.sort_by(|left, right| {
        right
            .active
            .cmp(&left.active)
            .then_with(|| left.name.to_ascii_lowercase().cmp(&right.name.to_ascii_lowercase()))
    });
    runtimes
}

fn read_game_preference(game_id: &str) -> Option<OpenXrGamePreference> {
    let path = preference_path(game_id).ok()?;
    let contents = fs::read_to_string(path).ok()?;
    serde_json::from_str(&contents).ok()
}

pub(crate) fn game_runtime_override(game_id: &str) -> Option<String> {
    let preference = read_game_preference(game_id)?;
    let path = PathBuf::from(&preference.manifest_path);
    path.is_file().then_some(preference.manifest_path)
}

fn runtime_name_for_path(runtimes: &[OpenXrRuntimeInfo], path: Option<&str>) -> Option<String> {
    let normalized = path.map(normalize_path)?;
    runtimes
        .iter()
        .find(|runtime| normalize_path(&runtime.manifest_path) == normalized)
        .map(|runtime| runtime.name.clone())
}

fn inspect_state(game_id: Option<&str>) -> Result<OpenXrState, String> {
    if let Some(game_id) = game_id {
        if !valid_game_id(game_id) {
            return Err("Invalid game id for OpenXR inspection.".to_owned());
        }
    }

    let active_runtime = system_active_runtime();
    let runtimes = discover_runtimes(active_runtime.as_deref());
    let stored_override = game_id.and_then(read_game_preference);
    let game_override = stored_override.as_ref().map(|value| value.manifest_path.clone());
    let valid_game_override = game_override
        .as_deref()
        .filter(|path| Path::new(path).is_file())
        .map(str::to_owned);
    let effective_runtime = valid_game_override
        .clone()
        .or_else(|| active_runtime.clone().filter(|path| Path::new(path).is_file()));
    let effective_source = if valid_game_override.is_some() {
        "game".to_owned()
    } else if effective_runtime.is_some() {
        "system".to_owned()
    } else {
        "none".to_owned()
    };

    let mut warnings = Vec::new();
    if active_runtime.is_none() {
        warnings.push("Windows does not currently define an active OpenXR runtime.".to_owned());
    } else if active_runtime
        .as_deref()
        .is_some_and(|path| !Path::new(path).is_file())
    {
        warnings.push("The Windows ActiveRuntime registry value points to a missing manifest file.".to_owned());
    }
    if game_override.is_some() && valid_game_override.is_none() {
        warnings.push(
            "The saved per-game OpenXR override points to a missing manifest and will be ignored until changed."
                .to_owned(),
        );
    }
    if runtimes.is_empty() {
        warnings.push("No installed OpenXR runtimes were discovered.".to_owned());
    }

    Ok(OpenXrState {
        active_runtime_name: runtime_name_for_path(&runtimes, active_runtime.as_deref()),
        game_override_name: runtime_name_for_path(&runtimes, game_override.as_deref()),
        effective_runtime_name: runtime_name_for_path(&runtimes, effective_runtime.as_deref()),
        active_runtime,
        game_override,
        effective_runtime,
        effective_source,
        runtimes,
        warnings,
    })
}

fn validate_runtime_choice(path: &str) -> Result<PathBuf, String> {
    let candidate = PathBuf::from(path);
    if !candidate.is_absolute() || !candidate.is_file() {
        return Err("Selected OpenXR runtime manifest does not exist.".to_owned());
    }
    if candidate
        .extension()
        .and_then(|value| value.to_str())
        .is_none_or(|extension| !extension.eq_ignore_ascii_case("json"))
    {
        return Err("OpenXR runtime manifest must be a JSON file.".to_owned());
    }

    let contents = fs::read_to_string(&candidate)
        .map_err(|error| format!("Could not read selected OpenXR manifest: {error}"))?;
    let json: Value = serde_json::from_str(&contents)
        .map_err(|error| format!("Selected OpenXR manifest is not valid JSON: {error}"))?;
    if json
        .pointer("/runtime/library_path")
        .and_then(Value::as_str)
        .is_none()
    {
        return Err("Selected JSON is not an OpenXR runtime manifest.".to_owned());
    }

    Ok(candidate)
}

fn write_game_preference(game_id: &str, manifest_path: Option<&str>) -> Result<(), String> {
    let path = preference_path(game_id)?;
    if let Some(manifest_path) = manifest_path {
        let manifest = validate_runtime_choice(manifest_path)?;
        let parent = path
            .parent()
            .ok_or_else(|| "Could not resolve OpenXR preference directory.".to_owned())?;
        fs::create_dir_all(parent)
            .map_err(|error| format!("Could not create OpenXR preference directory: {error}"))?;
        let value = OpenXrGamePreference {
            manifest_path: manifest.to_string_lossy().into_owned(),
        };
        let json = serde_json::to_string_pretty(&value)
            .map_err(|error| format!("Could not serialize OpenXR preference: {error}"))?;
        fs::write(&path, json)
            .map_err(|error| format!("Could not save OpenXR preference: {error}"))?;
    } else if path.exists() {
        fs::remove_file(&path)
            .map_err(|error| format!("Could not clear OpenXR preference: {error}"))?;
    }
    Ok(())
}

fn escape_powershell_single_quote(value: &str) -> String {
    value.replace(''', "''")
}

fn set_system_runtime_elevated(manifest_path: &Path) -> Result<(), String> {
    let manifest = manifest_path.to_string_lossy();
    let reg_arguments = format!(
        "add \"{}\" /v ActiveRuntime /t REG_SZ /d \"{}\" /f",
        OPENXR_REGISTRY_KEY, manifest
    );
    let escaped_arguments = escape_powershell_single_quote(&reg_arguments);
    let script = format!(
        "$p = Start-Process -FilePath \"$env:SystemRoot\\System32\\reg.exe\" -ArgumentList '{}' -Verb RunAs -Wait -PassThru; exit $p.ExitCode",
        escaped_arguments
    );

    let status = Command::new("powershell.exe")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &script])
        .status()
        .map_err(|error| format!("Could not request administrator permission for OpenXR: {error}"))?;

    if !status.success() {
        return Err(
            "OpenXR runtime was not changed. The administrator prompt may have been cancelled or Windows rejected the registry update."
                .to_owned(),
        );
    }

    let active = system_active_runtime().ok_or_else(|| {
        "Windows did not report an active OpenXR runtime after the registry update.".to_owned()
    })?;
    if normalize_path(&active) != normalize_path(&manifest) {
        return Err("Windows OpenXR ActiveRuntime did not match the selected runtime after the update.".to_owned());
    }
    Ok(())
}

#[tauri::command]
pub fn inspect_openxr(game_id: Option<String>) -> Result<OpenXrState, String> {
    inspect_state(game_id.as_deref())
}

#[tauri::command]
pub fn set_game_openxr_runtime(
    game_id: String,
    manifest_path: Option<String>,
) -> Result<OpenXrState, String> {
    write_game_preference(&game_id, manifest_path.as_deref())?;
    inspect_state(Some(&game_id))
}

#[tauri::command]
pub fn set_system_openxr_runtime(
    manifest_path: String,
    game_id: Option<String>,
) -> Result<OpenXrState, String> {
    let manifest = validate_runtime_choice(&manifest_path)?;
    set_system_runtime_elevated(&manifest)?;
    inspect_state(game_id.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_active_runtime_with_spaces() {
        let output = r#"
HKEY_LOCAL_MACHINE\SOFTWARE\Khronos\OpenXR\1
    ActiveRuntime    REG_SZ    C:\Program Files (x86)\Steam\steamapps\common\SteamVR\steamxr_win64.json
"#;
        assert_eq!(
            parse_active_runtime_output(output).as_deref(),
            Some(r"C:\Program Files (x86)\Steam\steamapps\common\SteamVR\steamxr_win64.json")
        );
    }

    #[test]
    fn parses_available_runtime_enabled_state() {
        let output = r#"
HKEY_LOCAL_MACHINE\SOFTWARE\Khronos\OpenXR\1\AvailableRuntimes
    C:\Runtime A\runtime.json    REG_DWORD    0x0
    C:\Runtime B\runtime.json    REG_DWORD    0x1
"#;
        let runtimes = parse_available_runtime_output(output);
        assert_eq!(runtimes.len(), 2);
        assert!(runtimes[0].1);
        assert!(!runtimes[1].1);
    }

    #[test]
    fn game_ids_cannot_escape_profile_directory() {
        assert!(valid_game_id("cyberpunk-2077"));
        assert!(valid_game_id("elden_ring"));
        assert!(!valid_game_id("../escape"));
        assert!(!valid_game_id("game\\escape"));
    }
}
