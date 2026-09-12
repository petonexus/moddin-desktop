use serde::Serialize;
use std::{
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
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
}

fn is_process_running(image_name: &str) -> bool {
    let output = Command::new("tasklist")
        .args(["/FI", &format!("IMAGENAME eq {image_name}"), "/FO", "CSV", "/NH"])
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

#[tauri::command]
pub fn inspect_game_environment(
    install_dir: String,
    executable: String,
) -> Result<GameEnvironmentInspection, String> {
    let root = PathBuf::from(&install_dir);
    if !root.is_dir() {
        return Err(format!("Game install directory does not exist: {install_dir}"));
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

        let size_bytes = candidate.metadata().map(|metadata| metadata.len()).unwrap_or(0);
        proxy_dlls.push(ProxyDllInfo {
            name: (*dll_name).to_owned(),
            path: candidate.to_string_lossy().into_owned(),
            size_bytes,
        });
    }

    Ok(GameEnvironmentInspection {
        executable_exists: executable_path.is_file(),
        game_running: is_process_running(process_name),
        executable_path: executable_path.to_string_lossy().into_owned(),
        executable_directory: executable_directory.to_string_lossy().into_owned(),
        proxy_dlls,
    })
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
}
