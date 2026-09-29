use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::{Component, Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::SystemTime,
};

const PROXY_DLL_NAMES: &[&str] = &[
    "dxgi.dll",
    "d3d11.dll",
    "d3d12.dll",
    "winmm.dll",
    "version.dll",
    "dinput8.dll",
    "xinput1_3.dll",
];

const OPTISCALER_MARKER_FILE: &str = ".moddin-optiscaler.json";

const MAX_EXECUTABLE_SCAN_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyDllInfo {
    pub name: String,
    pub path: String,
    pub size_bytes: u64,
}

/// Who currently occupies a proxy-DLL slot next to a game executable.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyOccupant {
    pub name: String,
    pub path: String,
    pub size_bytes: u64,
    pub managed_by_moddin: bool,
    pub held_by: String,
}

/// How a proxy-DLL conflict will be resolved when a module is applied.
///
/// Chain-loading research (verified 2026-09 against the OptiScaler wiki and
/// ReShade docs): a generic "chain" strategy was considered and rejected.
/// OptiScaler only special-cases `ReShade64.dll` (`LoadReshade=true`, plus
/// SpecialK), and ReShade forwards to the *system* copy of its own proxy
/// name — neither loader can be pointed at an arbitrary renamed proxy such
/// as `dxgi.dll.moddin-proxy`. Because there is no reliable contract for
/// "load the previous occupant under a custom name", resolution deliberately
/// stays limited to `use_next_free` + `replace_with_backup`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProxyResolution {
    /// No collision on the chosen proxy; the first free candidate is used.
    UseNextFree,
    /// The chosen proxy is occupied by a Moddin-managed DLL (or the caller
    /// explicitly accepted replacing an unknown one). The existing file is
    /// backed up by the install transaction and replaced, so Undo restores
    /// the previous DLL.
    ReplaceWithBackup,
    /// The chosen proxy is held by a third-party DLL and replacing it was
    /// not allowed; installation stays blocked.
    Blocked,
}

/// Rejects installs whose caller pinned a resolution that no longer matches
/// the environment (e.g. a stale preview replayed after the game directory
/// changed). `None` means "accept whatever the recomputed preview decides".
pub fn ensure_resolution_matches(
    expected: Option<ProxyResolution>,
    computed: ProxyResolution,
    module: &str,
) -> Result<(), String> {
    if let Some(expected) = expected {
        if expected != computed {
            return Err(format!(
                "{module} resolution '{expected:?}' no longer matches the game environment \
                 (recomputed '{computed:?}'). Re-run the preview and retry."
            ));
        }
    }
    Ok(())
}

/// Classify whatever occupies the proxy-DLL slot `name` inside
/// `executable_directory`. Returns `None` when the slot is free.
///
/// A file counts as managed by Moddin when a Moddin marker beside the
/// executable names it as the module proxy, or when an `applied` Moddin
/// transaction still holds a backup of that exact path (so replacing it is
/// recoverable via Undo). `reshade.rs` layers its own marker check on top
/// because the ReShade marker lives in the Moddin tool store, not beside
/// the game executable.
pub fn classify_proxy_occupant(executable_directory: &Path, name: &str) -> Option<ProxyOccupant> {
    let path = executable_directory.join(name);
    if !path.is_file() {
        return None;
    }

    let size_bytes = path
        .metadata()
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    let (managed_by_moddin, held_by) = proxy_occupant_owner(executable_directory, name, &path);

    Some(ProxyOccupant {
        name: name.to_owned(),
        path: path.to_string_lossy().into_owned(),
        size_bytes,
        managed_by_moddin,
        held_by,
    })
}

fn proxy_occupant_owner(executable_directory: &Path, name: &str, path: &Path) -> (bool, String) {
    if optiscaler_marker_owns_proxy(executable_directory, name) {
        return (
            true,
            "OptiScaler (managed by Moddin)".to_owned(),
        );
    }

    if transaction_store_holds_backup(path) {
        return (
            true,
            "Moddin transaction backup (previously replaced by Moddin)".to_owned(),
        );
    }

    (
        false,
        "another loader (not managed by Moddin)".to_owned(),
    )
}

fn optiscaler_marker_owns_proxy(executable_directory: &Path, name: &str) -> bool {
    let contents = match fs::read_to_string(executable_directory.join(OPTISCALER_MARKER_FILE)) {
        Ok(contents) => contents,
        Err(_) => return false,
    };
    let marker: serde_json::Value = match serde_json::from_str(&contents) {
        Ok(marker) => marker,
        Err(_) => return false,
    };

    let names_current_proxy = marker
        .get("proxyDll")
        .and_then(|value| value.as_str())
        .is_some_and(|proxy| proxy.eq_ignore_ascii_case(name));
    let lists_as_installed = marker
        .get("installedFiles")
        .and_then(|value| value.as_array())
        .is_some_and(|files| {
            files.iter().any(|file| {
                file.as_str()
                    .is_some_and(|file| file.eq_ignore_ascii_case(name))
            })
        });

    names_current_proxy || lists_as_installed
}

fn transaction_store_holds_backup(path: &Path) -> bool {
    let records = match crate::transaction::list_transactions_sync() {
        Ok(records) => records,
        Err(_) => return false,
    };
    transaction_records_holds_backup(&records, path)
}

fn transaction_records_holds_backup(
    records: &[crate::transaction::TransactionRecord],
    path: &Path,
) -> bool {
    let needle = path.to_string_lossy().to_ascii_lowercase();
    records.iter().any(|record| {
        record.status == "applied"
            && record.files.iter().any(|file| {
                file.existed_before
                    && file.target_path.to_ascii_lowercase() == needle
                    && file
                        .backup_path
                        .as_ref()
                        .is_some_and(|backup| Path::new(backup).is_file())
            })
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameEnvironmentInspection {
    pub executable_path: String,
    pub executable_directory: String,
    pub executable_exists: bool,
    pub game_running: bool,
    pub proxy_dlls: Vec<ProxyDllInfo>,
    pub engine: Option<String>,
    pub engine_version: Option<String>,
    pub engine_confidence: String,
    pub engine_evidence: Vec<String>,
}

#[derive(Debug, Clone)]
struct EngineDetection {
    engine: Option<String>,
    version: Option<String>,
    confidence: String,
    evidence: Vec<String>,
}

#[derive(Debug, Clone)]
struct EngineCacheEntry {
    size: u64,
    modified: Option<SystemTime>,
    detection: EngineDetection,
}

static ENGINE_CACHE: OnceLock<Mutex<HashMap<PathBuf, EngineCacheEntry>>> = OnceLock::new();

fn is_process_running(image_name: &str) -> bool {
    crate::process::is_process_running(image_name)
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

fn contains_ascii_case_insensitive(haystack: &[u8], needle: &str) -> bool {
    let needle = needle.as_bytes();
    if needle.is_empty() || needle.len() > haystack.len() {
        return false;
    }

    haystack.windows(needle.len()).any(|window| {
        window
            .iter()
            .zip(needle)
            .all(|(left, right)| left.to_ascii_lowercase() == right.to_ascii_lowercase())
    })
}

fn contains_utf16le_case_insensitive(haystack: &[u8], needle: &str) -> bool {
    let needle: Vec<u8> = needle
        .encode_utf16()
        .flat_map(|unit| unit.to_le_bytes())
        .collect();
    if needle.is_empty() || needle.len() > haystack.len() {
        return false;
    }

    haystack.windows(needle.len()).any(|window| {
        window
            .iter()
            .zip(&needle)
            .all(|(left, right)| left.to_ascii_lowercase() == right.to_ascii_lowercase())
    })
}

fn contains_marker(bytes: &[u8], marker: &str) -> bool {
    contains_ascii_case_insensitive(bytes, marker)
        || contains_utf16le_case_insensitive(bytes, marker)
}

fn scan_executable(path: &Path) -> Vec<u8> {
    let Ok(mut file) = File::open(path) else {
        return Vec::new();
    };

    let _ = file.seek(SeekFrom::Start(0));
    let mut bytes = Vec::new();
    let _ = file.take(MAX_EXECUTABLE_SCAN_BYTES).read_to_end(&mut bytes);
    bytes
}

fn engine_detection_from_signals(
    executable_name: &str,
    bytes: &[u8],
    unreal_layout: bool,
    unity_layout: bool,
    re_engine_layout: bool,
    red_engine_layout: bool,
) -> EngineDetection {
    let mut evidence = Vec::new();

    let unreal_signature = [
        "Unreal Engine",
        "UE4Game",
        "UE5Game",
        "/Script/Engine",
        "FEngineVersion",
    ]
    .iter()
    .find(|marker| contains_marker(bytes, marker));
    let unity_signature = ["UnityPlayer", "globalgamemanagers", "il2cpp"]
        .iter()
        .find(|marker| contains_marker(bytes, marker));
    let red_engine_signature = ["REDengine", "Cyberpunk2077"]
        .iter()
        .find(|marker| contains_marker(bytes, marker));
    let re_engine_signature = ["RE Engine", "re_chunk_000.pak", "REFramework"]
        .iter()
        .find(|marker| contains_marker(bytes, marker));

    if unity_layout || unity_signature.is_some() {
        if unity_layout {
            evidence.push("UnityPlayer.dll/globalgamemanagers layout found".to_owned());
        }
        if let Some(marker) = unity_signature {
            evidence.push(format!("Unity marker found in {executable_name}: {marker}"));
        }
        return EngineDetection {
            engine: Some("Unity".to_owned()),
            version: None,
            confidence: if unity_layout { "high" } else { "medium" }.to_owned(),
            evidence,
        };
    }

    if unreal_layout || unreal_signature.is_some() {
        if unreal_layout {
            evidence.push("Unreal Binaries/Win64 + Content/Paks layout found".to_owned());
        }
        if let Some(marker) = unreal_signature {
            evidence.push(format!(
                "Unreal marker found in {executable_name}: {marker}"
            ));
        }
        let version = if contains_marker(bytes, "UE5") {
            evidence.push("UE5 marker found".to_owned());
            Some("UE5".to_owned())
        } else if contains_marker(bytes, "UE4") {
            evidence.push("UE4 marker found".to_owned());
            Some("UE4".to_owned())
        } else {
            None
        };
        return EngineDetection {
            engine: Some("Unreal Engine".to_owned()),
            version,
            confidence: if unreal_signature.is_some() && unreal_layout {
                "high"
            } else {
                "medium"
            }
            .to_owned(),
            evidence,
        };
    }

    if re_engine_layout || re_engine_signature.is_some() {
        if re_engine_layout {
            evidence.push("RE Engine re_chunk package layout found".to_owned());
        }
        if let Some(marker) = re_engine_signature {
            evidence.push(format!(
                "RE Engine marker found in {executable_name}: {marker}"
            ));
        }
        return EngineDetection {
            engine: Some("RE Engine".to_owned()),
            version: None,
            confidence: if re_engine_layout { "high" } else { "medium" }.to_owned(),
            evidence,
        };
    }

    if red_engine_layout || red_engine_signature.is_some() {
        if red_engine_layout {
            evidence.push("REDengine archive/pc/content + bin/x64 layout found".to_owned());
        }
        if let Some(marker) = red_engine_signature {
            evidence.push(format!(
                "REDengine marker found in {executable_name}: {marker}"
            ));
        }
        return EngineDetection {
            engine: Some("REDengine".to_owned()),
            version: None,
            confidence: if red_engine_layout { "high" } else { "medium" }.to_owned(),
            evidence,
        };
    }

    let known_engine_signatures = [
        ("Frostbite", ["Frostbite"].as_slice()),
        ("Snowdrop", ["Snowdrop"].as_slice()),
        (
            "Creation Engine",
            ["Creation Engine", "Gamebryo"].as_slice(),
        ),
        ("id Tech", ["id Tech"].as_slice()),
        ("Source", ["Source Engine", "Source 2"].as_slice()),
        ("Dunia", ["Dunia"].as_slice()),
        ("Anvil", ["AnvilNext", "Anvil"].as_slice()),
        ("Decima", ["Decima"].as_slice()),
        ("CryEngine", ["CryEngine"].as_slice()),
    ];
    if let Some((engine, marker)) = known_engine_signatures
        .iter()
        .find_map(|(engine, markers)| {
            markers
                .iter()
                .find(|marker| contains_marker(bytes, marker))
                .map(|marker| (*engine, *marker))
        })
    {
        evidence.push(format!(
            "{engine} marker found in {executable_name}: {marker}"
        ));
        return EngineDetection {
            engine: Some(engine.to_owned()),
            version: None,
            confidence: "medium".to_owned(),
            evidence,
        };
    }

    EngineDetection {
        engine: None,
        version: None,
        confidence: "unknown".to_owned(),
        evidence: vec![
            "No trusted engine marker was found in the executable or known game layout.".to_owned(),
        ],
    }
}

fn detect_engine_uncached(root: &Path, executable_path: &Path) -> EngineDetection {
    let executable_name = executable_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("game.exe");
    let unreal_layout = has_unreal_layout(root, executable_path);
    let unity_layout = root.join("UnityPlayer.dll").is_file()
        || fs::read_dir(root).ok().is_some_and(|entries| {
            entries.flatten().any(|entry| {
                entry.file_name().to_string_lossy().ends_with("_Data")
                    && entry.path().join("globalgamemanagers").is_file()
            })
        });
    let re_engine_layout = root.join("re_chunk_000.pak").is_file()
        || root.join("re_chunk_000.pak.patch_001.pak").is_file();
    let red_engine_layout = root.join("archive").join("pc").join("content").is_dir()
        && root.join("bin").join("x64").is_dir();
    let bytes = if unreal_layout || unity_layout || re_engine_layout || red_engine_layout {
        Vec::new()
    } else {
        scan_executable(executable_path)
    };

    engine_detection_from_signals(
        executable_name,
        &bytes,
        unreal_layout,
        unity_layout,
        re_engine_layout,
        red_engine_layout,
    )
}

fn has_unreal_layout(root: &Path, executable_path: &Path) -> bool {
    let mut candidate = Some(root);
    while let Some(path) = candidate {
        if path.join("Binaries").join("Win64").is_dir()
            && path.join("Content").join("Paks").is_dir()
        {
            return true;
        }
        candidate = path.parent();
    }

    let mut candidate = executable_path.parent();
    while let Some(path) = candidate {
        if path.join("Binaries").join("Win64").is_dir()
            && path.join("Content").join("Paks").is_dir()
        {
            return true;
        }
        candidate = path.parent();
    }

    false
}

fn detect_engine(root: &Path, executable_path: &Path) -> EngineDetection {
    let metadata = fs::metadata(executable_path).ok();
    let size = metadata.as_ref().map(|value| value.len()).unwrap_or(0);
    let modified = metadata.and_then(|value| value.modified().ok());
    let cache = ENGINE_CACHE.get_or_init(|| Mutex::new(HashMap::new()));

    if let Ok(mut entries) = cache.lock() {
        if let Some(entry) = entries.get(executable_path) {
            if entry.size == size && entry.modified == modified {
                return entry.detection.clone();
            }
        }

        let detection = detect_engine_uncached(root, executable_path);
        entries.insert(
            executable_path.to_path_buf(),
            EngineCacheEntry {
                size,
                modified,
                detection: detection.clone(),
            },
        );
        return detection;
    }

    detect_engine_uncached(root, executable_path)
}

pub fn inspect_game_environment_sync(
    install_dir: String,
    executable: String,
) -> Result<GameEnvironmentInspection, String> {
    let root = PathBuf::from(&install_dir);
    if !root.is_dir() {
        return Err(format!(
            "Game install directory does not exist: {install_dir}"
        ));
    }

    let executable_path = safe_join_relative(&root, &executable)?;
    let executable_directory = executable_path
        .parent()
        .ok_or_else(|| "Could not resolve executable directory.".to_owned())?
        .to_path_buf();
    let process_name = executable_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "Could not resolve executable process name.".to_owned())?;

    let mut proxy_dlls = Vec::new();
    for dll_name in PROXY_DLL_NAMES {
        let candidate = executable_directory.join(dll_name);
        if !candidate.is_file() {
            continue;
        }

        let size_bytes = candidate
            .metadata()
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        proxy_dlls.push(ProxyDllInfo {
            name: (*dll_name).to_owned(),
            path: candidate.to_string_lossy().into_owned(),
            size_bytes,
        });
    }

    let engine_detection = detect_engine(&root, &executable_path);

    Ok(GameEnvironmentInspection {
        executable_exists: executable_path.is_file(),
        game_running: is_process_running(process_name),
        executable_path: executable_path.to_string_lossy().into_owned(),
        executable_directory: executable_directory.to_string_lossy().into_owned(),
        proxy_dlls,
        engine: engine_detection.engine,
        engine_version: engine_detection.version,
        engine_confidence: engine_detection.confidence,
        engine_evidence: engine_detection.evidence,
    })
}

#[tauri::command]
pub async fn inspect_game_environment(
    install_dir: String,
    executable: String,
) -> Result<GameEnvironmentInspection, String> {
    tauri::async_runtime::spawn_blocking(move || {
        inspect_game_environment_sync(install_dir, executable)
    })
    .await
    .map_err(|error| format!("Game inspection task failed: {error}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_paths_that_escape_game_root() {
        let root = Path::new(r"C:\Games\Example");
        assert!(safe_join_relative(root, r"..\other\tool.exe").is_err());
        assert!(safe_join_relative(root, r"C:\Windows\notepad.exe").is_err());
    }

    #[test]
    fn accepts_nested_relative_executable() {
        let root = Path::new(r"D:\SteamLibrary\steamapps\common\Example");
        let joined = safe_join_relative(root, r"bin\x64\game.exe").expect("safe path");
        assert_eq!(joined, root.join(r"bin\x64\game.exe"));
    }

    #[test]
    fn detects_unreal_layout_below_install_root() {
        let suffix = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("moddin-inspection-{suffix}"));
        let nested_root = root.join("DeadIsland");
        fs::create_dir_all(nested_root.join("Binaries").join("Win64")).expect("binaries");
        fs::create_dir_all(nested_root.join("Content").join("Paks")).expect("content");

        let executable = nested_root
            .join("Binaries")
            .join("Win64")
            .join("DeadIsland-Win64-Shipping.exe");
        assert!(has_unreal_layout(&root, &executable));

        fs::remove_dir_all(root).expect("temporary inspection fixture cleanup");
    }

    #[test]
    fn detects_unreal_from_binary_and_layout_signals() {
        let result = engine_detection_from_signals(
            "Game-Win64-Shipping.exe",
            b"... Unreal Engine ... UE5 ...",
            true,
            false,
            false,
            false,
        );
        assert_eq!(result.engine.as_deref(), Some("Unreal Engine"));
        assert_eq!(result.version.as_deref(), Some("UE5"));
        assert_eq!(result.confidence, "high");
    }

    #[test]
    fn detects_unity_before_generic_unknown() {
        let result = engine_detection_from_signals(
            "Game.exe",
            b"UnityPlayer globalgamemanagers",
            false,
            true,
            false,
            false,
        );
        assert_eq!(result.engine.as_deref(), Some("Unity"));
        assert_eq!(result.confidence, "high");
    }

    #[test]
    fn does_not_guess_an_engine_without_signals() {
        let result =
            engine_detection_from_signals("Game.exe", b"game data", false, false, false, false);
        assert_eq!(result.engine, None);
        assert_eq!(result.confidence, "unknown");
    }

    #[test]
    fn detects_redengine_from_cyberpunk_layout() {
        let result =
            engine_detection_from_signals("Cyberpunk2077.exe", b"", false, false, false, true);
        assert_eq!(result.engine.as_deref(), Some("REDengine"));
        assert_eq!(result.confidence, "high");
    }

    fn temp_fixture_dir(tag: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("moddin-proxy-{tag}-{suffix}"));
        fs::create_dir_all(&dir).expect("fixture directory");
        dir
    }

    #[test]
    fn proxy_resolution_serializes_snake_case_contract() {
        assert_eq!(
            serde_json::to_string(&ProxyResolution::UseNextFree).unwrap(),
            "\"use_next_free\""
        );
        assert_eq!(
            serde_json::to_string(&ProxyResolution::ReplaceWithBackup).unwrap(),
            "\"replace_with_backup\""
        );
        assert_eq!(
            serde_json::to_string(&ProxyResolution::Blocked).unwrap(),
            "\"blocked\""
        );
        let parsed: ProxyResolution = serde_json::from_str("\"replace_with_backup\"").unwrap();
        assert_eq!(parsed, ProxyResolution::ReplaceWithBackup);
    }

    #[test]
    fn ensure_resolution_matches_only_rejects_stale_pins() {
        assert!(ensure_resolution_matches(None, ProxyResolution::Blocked, "Test").is_ok());
        assert!(ensure_resolution_matches(
            Some(ProxyResolution::ReplaceWithBackup),
            ProxyResolution::ReplaceWithBackup,
            "Test"
        )
        .is_ok());
        assert!(ensure_resolution_matches(
            Some(ProxyResolution::UseNextFree),
            ProxyResolution::Blocked,
            "Test"
        )
        .is_err());
    }

    #[test]
    fn classify_returns_none_for_free_slot() {
        let dir = temp_fixture_dir("free");
        assert!(classify_proxy_occupant(&dir, "dxgi.dll").is_none());
        fs::remove_dir_all(dir).expect("fixture cleanup");
    }

    #[test]
    fn classify_marks_unknown_occupant() {
        let dir = temp_fixture_dir("unknown");
        fs::write(dir.join("dxgi.dll"), b"12345").expect("occupant");

        let occupant = classify_proxy_occupant(&dir, "dxgi.dll").expect("occupant");
        assert_eq!(occupant.size_bytes, 5);
        assert!(!occupant.managed_by_moddin);
        assert!(occupant.held_by.contains("not managed"));

        fs::remove_dir_all(dir).expect("fixture cleanup");
    }

    #[test]
    fn classify_marks_optiscaler_marker_owned_proxy() {
        let dir = temp_fixture_dir("managed");
        fs::write(dir.join("dxgi.dll"), b"optiscaler").expect("occupant");
        fs::write(
            dir.join(OPTISCALER_MARKER_FILE),
            r#"{"version":"0.9.4","proxyDll":"dxgi.dll","sourceSha256":"abc","installedFiles":["dxgi.dll"]}"#,
        )
        .expect("marker");

        let occupant = classify_proxy_occupant(&dir, "dxgi.dll").expect("occupant");
        assert!(occupant.managed_by_moddin);
        assert!(occupant.held_by.contains("OptiScaler"));

        fs::remove_dir_all(dir).expect("fixture cleanup");
    }

    #[test]
    fn classify_ignores_marker_for_other_proxy() {
        let dir = temp_fixture_dir("other-proxy");
        fs::write(dir.join("dxgi.dll"), b"third-party").expect("occupant");
        fs::write(dir.join("winmm.dll"), b"optiscaler").expect("occupant");
        fs::write(
            dir.join(OPTISCALER_MARKER_FILE),
            r#"{"version":"0.9.4","proxyDll":"winmm.dll","sourceSha256":"abc","installedFiles":["winmm.dll"]}"#,
        )
        .expect("marker");

        let dxgi = classify_proxy_occupant(&dir, "dxgi.dll").expect("occupant");
        assert!(!dxgi.managed_by_moddin);
        let winmm = classify_proxy_occupant(&dir, "winmm.dll").expect("occupant");
        assert!(winmm.managed_by_moddin);

        fs::remove_dir_all(dir).expect("fixture cleanup");
    }

    #[test]
    fn transaction_records_holds_backup_requires_applied_live_backup() {
        let dir = temp_fixture_dir("tx-backup");
        let backup = dir.join("backup-dxgi.dll");
        fs::write(&backup, b"original").expect("backup file");

        let record = |status: &str, target: &str, existed_before: bool, backup_path: Option<String>| {
            crate::transaction::TransactionRecord {
                id: "1757720000000-0123456789abcdef0123456789abcdef".to_owned(),
                created_at: 1,
                kind: "optiscaler".to_owned(),
                label: "test".to_owned(),
                game_id: "game".to_owned(),
                target_path: "target".to_owned(),
                backup_path: "backup".to_owned(),
                status: status.to_owned(),
                files: vec![crate::transaction::TransactionFile {
                    target_path: target.to_owned(),
                    backup_path,
                    existed_before,
                }],
                created_directories: Vec::new(),
                metadata: Default::default(),
            }
        };
        let backup_string = Some(backup.to_string_lossy().into_owned());
        let needle = Path::new(r"C:\Games\Example\dxgi.dll");

        let hit = record("applied", r"C:\Games\Example\DXGI.dll", true, backup_string.clone());
        assert!(transaction_records_holds_backup(&[hit], needle));

        let rolled_back = record("rolled_back", r"C:\Games\Example\dxgi.dll", true, backup_string.clone());
        assert!(!transaction_records_holds_backup(&[rolled_back], needle));

        let missing_backup = dir.join("missing.dll");
        let missing = record(
            "applied",
            r"C:\Games\Example\dxgi.dll",
            true,
            Some(missing_backup.to_string_lossy().into_owned()),
        );
        assert!(!transaction_records_holds_backup(&[missing], needle));

        let other_path = record("applied", r"C:\Games\Example\winmm.dll", true, backup_string);
        assert!(!transaction_records_holds_backup(&[other_path], needle));

        let not_existed_before = record("applied", r"C:\Games\Example\dxgi.dll", false, None);
        assert!(!transaction_records_holds_backup(&[not_existed_before], needle));

        fs::remove_dir_all(dir).expect("fixture cleanup");
    }
}
