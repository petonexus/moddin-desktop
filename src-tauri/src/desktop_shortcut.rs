use crate::process::HideConsole;
use crate::transaction::{self, TransactionRecord};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    env, fs,
    path::{Component, Path, PathBuf},
    process::Command,
};
use tauri::Manager;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopShortcutRequest {
    pub game_id: String,
    pub game_name: String,
    pub install_dir: String,
    pub executable: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopShortcutPreview {
    pub can_apply: bool,
    pub shortcut_path: String,
    pub target_path: String,
    pub icon_path: String,
    pub will_replace: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopShortcutResult {
    pub shortcut_path: String,
    pub transaction: TransactionRecord,
}

struct ShortcutContext {
    desktop_dir: PathBuf,
    shortcut_path: PathBuf,
    target_path: PathBuf,
    game_name: String,
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
                    "Catalog executable path must stay inside the game directory: {relative}"
                ));
            }
        }
    }

    Ok(root.join(relative_path))
}

fn valid_game_id(game_id: &str) -> bool {
    !game_id.is_empty()
        && game_id.len() <= 96
        && game_id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
}

fn shortcut_stem(game_name: &str) -> Result<String, String> {
    let stem = game_name
        .chars()
        .map(|character| {
            if matches!(character, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') {
                ' '
            } else {
                character
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    if stem.is_empty() || stem.len() > 120 || stem == "." || stem == ".." {
        return Err("Game name is not valid for a desktop shortcut.".to_owned());
    }

    Ok(stem)
}

fn shortcut_context(
    app: &tauri::AppHandle,
    request: &DesktopShortcutRequest,
) -> Result<ShortcutContext, String> {
    if !valid_game_id(&request.game_id) {
        return Err("Desktop shortcut request has an invalid game id.".to_owned());
    }

    let install_dir = PathBuf::from(&request.install_dir);
    if !install_dir.is_dir() {
        return Err(format!(
            "Game install directory does not exist: {}",
            request.install_dir
        ));
    }

    let target_path = safe_join_relative(&install_dir, &request.executable)?;
    let game_name = shortcut_stem(&request.game_name)?;
    let desktop_dir = app
        .path()
        .desktop_dir()
        .map_err(|error| format!("Could not resolve the Windows Desktop directory: {error}"))?;
    let shortcut_path = desktop_dir.join(format!("{game_name} - Moddin.lnk"));

    Ok(ShortcutContext {
        desktop_dir,
        shortcut_path,
        target_path,
        game_name,
    })
}

fn preview_inner(
    app: &tauri::AppHandle,
    request: &DesktopShortcutRequest,
) -> Result<DesktopShortcutPreview, String> {
    let context = shortcut_context(app, request)?;
    let executable_exists = context.target_path.is_file();

    Ok(DesktopShortcutPreview {
        can_apply: executable_exists && context.desktop_dir.is_dir(),
        shortcut_path: context.shortcut_path.to_string_lossy().into_owned(),
        target_path: context.target_path.to_string_lossy().into_owned(),
        icon_path: context.target_path.to_string_lossy().into_owned(),
        will_replace: context.shortcut_path.is_file(),
    })
}

fn powershell_path() -> PathBuf {
    env::var_os("WINDIR")
        .map(PathBuf::from)
        .map(|root| root.join(r"System32\WindowsPowerShell\v1.0\powershell.exe"))
        .filter(|path| path.is_file())
        .unwrap_or_else(|| PathBuf::from("powershell.exe"))
}

fn create_windows_shortcut(context: &ShortcutContext) -> Result<(), String> {
    const SCRIPT: &str = r#"
param(
  [Parameter(Mandatory = $true)][string]$ShortcutPath,
  [Parameter(Mandatory = $true)][string]$TargetPath,
  [Parameter(Mandatory = $true)][string]$WorkingDirectory,
  [Parameter(Mandatory = $true)][string]$Description
)
$ErrorActionPreference = 'Stop'
if (-not (Test-Path -LiteralPath $TargetPath -PathType Leaf)) {
  throw "Target executable was not found: $TargetPath"
}
$shell = New-Object -ComObject WScript.Shell
$shortcut = $shell.CreateShortcut($ShortcutPath)
$shortcut.TargetPath = $TargetPath
$shortcut.WorkingDirectory = $WorkingDirectory
$shortcut.Description = $Description
$shortcut.IconLocation = "$TargetPath,0"
$shortcut.Save()
"#;

    let script_path = env::temp_dir().join(format!(
        "moddin-create-desktop-shortcut-{}.ps1",
        uuid::Uuid::new_v4()
    ));
    fs::write(&script_path, SCRIPT)
        .map_err(|error| format!("Could not prepare the Windows shortcut helper: {error}"))?;

    let working_directory = context
        .target_path
        .parent()
        .ok_or_else(|| "Could not resolve the game executable directory.".to_owned())?;
    let output = Command::new(powershell_path())
        .hide_console()
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(&script_path)
        .arg(&context.shortcut_path)
        .arg(&context.target_path)
        .arg(working_directory)
        .arg(format!("Start {} from Moddin.", context.game_name))
        .output();
    let _ = fs::remove_file(&script_path);

    let output =
        output.map_err(|error| format!("Could not start the Windows shortcut helper: {error}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(if detail.is_empty() {
            "Windows could not create the desktop shortcut.".to_owned()
        } else {
            format!("Windows could not create the desktop shortcut: {detail}")
        });
    }

    if !context.shortcut_path.is_file() {
        return Err("Windows reported success but the desktop shortcut was not created.".to_owned());
    }

    Ok(())
}

#[tauri::command]
pub fn preview_desktop_shortcut(
    app: tauri::AppHandle,
    request: DesktopShortcutRequest,
) -> Result<DesktopShortcutPreview, String> {
    preview_inner(&app, &request)
}

#[tauri::command]
pub fn create_desktop_shortcut(
    app: tauri::AppHandle,
    request: DesktopShortcutRequest,
) -> Result<DesktopShortcutResult, String> {
    let context = shortcut_context(&app, &request)?;
    if !context.target_path.is_file() {
        return Err(format!(
            "Game executable was not found: {}",
            context.target_path.display()
        ));
    }
    if !context.desktop_dir.is_dir() {
        return Err(format!(
            "Windows Desktop directory does not exist: {}",
            context.desktop_dir.display()
        ));
    }

    let mut metadata = BTreeMap::new();
    metadata.insert(
        "targetPath".to_owned(),
        context.target_path.to_string_lossy().into_owned(),
    );
    metadata.insert(
        "iconPath".to_owned(),
        context.target_path.to_string_lossy().into_owned(),
    );
    let transaction = transaction::begin_file_set_transaction(
        &context.desktop_dir,
        &[context.shortcut_path.clone()],
        "desktop-shortcut",
        &format!("Create desktop shortcut for {}", context.game_name),
        &request.game_id,
        metadata,
    )?;

    if let Err(error) = create_windows_shortcut(&context) {
        let _ = transaction::restore_record(transaction);
        return Err(error);
    }

    let transaction = match transaction::mark_applied(transaction.clone()) {
        Ok(record) => record,
        Err(error) => {
            let _ = transaction::restore_record(transaction);
            return Err(format!(
                "Moddin could not save the shortcut rollback record, so the shortcut was restored: {error}"
            ));
        }
    };

    Ok(DesktopShortcutResult {
        shortcut_path: context.shortcut_path.to_string_lossy().into_owned(),
        transaction,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_executable_paths_that_escape_the_game_directory() {
        let root = Path::new(r"C:\Games\Example");
        assert!(safe_join_relative(root, r"..\outside.exe").is_err());
        assert!(safe_join_relative(root, r"C:\Windows\notepad.exe").is_err());
    }

    #[test]
    fn normalizes_windows_shortcut_names() {
        assert_eq!(shortcut_stem("DOOM (2016)").unwrap(), "DOOM (2016)");
        assert_eq!(shortcut_stem("Bad:name").unwrap(), "Bad name");
        assert!(shortcut_stem("..").is_err());
    }

    #[test]
    fn writes_a_windows_shortcut_from_the_verified_executable() {
        let target_path = powershell_path();
        assert!(target_path.is_file());

        let root = env::temp_dir().join(format!(
            "moddin-desktop-shortcut-test-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&root).unwrap();
        let shortcut_path = root.join("PowerShell - Moddin.lnk");
        let context = ShortcutContext {
            desktop_dir: root.clone(),
            shortcut_path: shortcut_path.clone(),
            target_path,
            game_name: "PowerShell".to_owned(),
        };

        create_windows_shortcut(&context).unwrap();
        assert!(shortcut_path.is_file());

        fs::remove_file(&shortcut_path).unwrap();
        fs::remove_dir(&root).unwrap();
    }
}
