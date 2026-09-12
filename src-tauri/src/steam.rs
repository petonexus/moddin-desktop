use serde::Serialize;
use std::{
    collections::HashSet,
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledGame {
    pub app_id: String,
    pub name: String,
    pub install_dir: String,
    pub library_path: String,
}

fn detect_steam_games_sync() -> Result<Vec<InstalledGame>, String> {
    let steam_roots = discover_steam_roots();
    let mut libraries = Vec::new();

    for root in steam_roots {
        push_unique_path(&mut libraries, root.clone());

        let library_file = root.join("steamapps").join("libraryfolders.vdf");
        if let Ok(contents) = fs::read_to_string(library_file) {
            for line in contents.lines() {
                if let Some((key, value)) = quoted_pair(line) {
                    if key == "path" {
                        push_unique_path(&mut libraries, PathBuf::from(unescape_vdf_path(&value)));
                    }
                }
            }
        }
    }

    let mut games = Vec::new();
    let mut seen_app_ids = HashSet::new();

    for library in libraries {
        let steamapps = library.join("steamapps");
        let Ok(entries) = fs::read_dir(&steamapps) else {
            continue;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };

            if !file_name.starts_with("appmanifest_") || !file_name.ends_with(".acf") {
                continue;
            }

            let Ok(contents) = fs::read_to_string(&path) else {
                continue;
            };

            let Some(app_id) = vdf_value(&contents, "appid") else {
                continue;
            };
            if !seen_app_ids.insert(app_id.clone()) {
                continue;
            }

            let Some(name) = vdf_value(&contents, "name") else {
                continue;
            };
            let Some(install_dir_name) = vdf_value(&contents, "installdir") else {
                continue;
            };

            let install_dir = steamapps.join("common").join(install_dir_name);
            games.push(InstalledGame {
                app_id,
                name,
                install_dir: path_to_string(&install_dir),
                library_path: path_to_string(&library),
            });
        }
    }

    games.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(games)
}

#[tauri::command]
pub async fn detect_steam_games() -> Result<Vec<InstalledGame>, String> {
    tauri::async_runtime::spawn_blocking(detect_steam_games_sync)
        .await
        .map_err(|error| format!("Steam library scan task failed: {error}"))?
}

fn discover_steam_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if let Ok(explicit) = env::var("STEAM_PATH") {
        push_existing_unique_path(&mut roots, PathBuf::from(explicit));
    }

    for registry_path in registry_steam_paths() {
        push_existing_unique_path(&mut roots, registry_path);
    }

    if let Ok(program_files_x86) = env::var("ProgramFiles(x86)") {
        push_existing_unique_path(&mut roots, PathBuf::from(program_files_x86).join("Steam"));
    }

    if let Ok(program_files) = env::var("ProgramFiles") {
        push_existing_unique_path(&mut roots, PathBuf::from(program_files).join("Steam"));
    }

    push_existing_unique_path(&mut roots, PathBuf::from(r"C:\Program Files (x86)\Steam"));
    push_existing_unique_path(&mut roots, PathBuf::from(r"C:\Program Files\Steam"));

    roots
}

fn registry_steam_paths() -> Vec<PathBuf> {
    let locations = [
        (r"HKCU\Software\Valve\Steam", "SteamPath"),
        (r"HKLM\SOFTWARE\WOW6432Node\Valve\Steam", "InstallPath"),
        (r"HKLM\SOFTWARE\Valve\Steam", "InstallPath"),
    ];

    let mut paths = Vec::new();
    for (key, value_name) in locations {
        let output = Command::new("reg.exe")
            .args(["query", key, "/v", value_name])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output();

        let Ok(output) = output else {
            continue;
        };
        if !output.status.success() {
            continue;
        }

        if let Some(value) =
            parse_registry_string(&String::from_utf8_lossy(&output.stdout), value_name)
        {
            push_unique_path(&mut paths, PathBuf::from(value));
        }
    }

    paths
}

fn parse_registry_string(output: &str, value_name: &str) -> Option<String> {
    output.lines().find_map(|line| {
        if !line.contains(value_name) {
            return None;
        }
        let marker = "REG_SZ";
        let marker_index = line.find(marker)?;
        let value = line[marker_index + marker.len()..].trim();
        (!value.is_empty()).then(|| value.to_owned())
    })
}

fn push_existing_unique_path(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if path.exists() {
        push_unique_path(paths, path);
    }
}

fn push_unique_path(paths: &mut Vec<PathBuf>, path: PathBuf) {
    let candidate = normalized_path_key(&path);
    if paths
        .iter()
        .any(|existing| normalized_path_key(existing) == candidate)
    {
        return;
    }
    paths.push(path);
}

fn normalized_path_key(path: &Path) -> String {
    path_to_string(path)
        .trim_end_matches(|c| c == '\\' || c == '/')
        .to_lowercase()
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn vdf_value(contents: &str, wanted_key: &str) -> Option<String> {
    contents.lines().find_map(|line| {
        let (key, value) = quoted_pair(line)?;
        (key == wanted_key).then_some(value)
    })
}

fn quoted_pair(line: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = line.split('"').collect();
    if parts.len() < 4 {
        return None;
    }

    Some((parts[1].to_owned(), parts[3].to_owned()))
}

fn unescape_vdf_path(path: &str) -> String {
    path.replace("\\\\", "\\")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_vdf_pairs() {
        let line = "\"path\"\t\t\"D:\\\\SteamLibrary\"";
        let pair = quoted_pair(line).expect("pair");
        assert_eq!(pair.0, "path");
        assert_eq!(unescape_vdf_path(&pair.1), r"D:\SteamLibrary");
    }

    #[test]
    fn reads_manifest_values() {
        let manifest = "\"AppState\"\n{\n  \"appid\"  \"1245620\"\n  \"name\"  \"ELDEN RING\"\n}";
        assert_eq!(vdf_value(manifest, "appid").as_deref(), Some("1245620"));
        assert_eq!(vdf_value(manifest, "name").as_deref(), Some("ELDEN RING"));
    }

    #[test]
    fn parses_registry_paths_with_spaces() {
        let output = r#"
HKEY_CURRENT_USER\Software\Valve\Steam
    SteamPath    REG_SZ    C:\Program Files (x86)\Steam
"#;
        assert_eq!(
            parse_registry_string(output, "SteamPath").as_deref(),
            Some(r"C:\Program Files (x86)\Steam")
        );
    }
}
