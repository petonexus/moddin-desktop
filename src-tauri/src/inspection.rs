use serde::Serialize;
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

const MAX_EXECUTABLE_SCAN_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyDllInfo {
    pub name: String,
    pub path: String,
    pub size_bytes: u64,
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
}
