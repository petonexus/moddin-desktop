//! GOG game discovery (Windows-only).
//!
//! GOG.com standalone installers write per-game registry entries under
//! `HKLM\SOFTWARE\WOW6432Node\GOG.com\Games\<gameId>` (and the matching
//! `HKCU\Software\Classes\VirtualStore\...` view for non-elevated
//! readers). Each entry carries the `path`, `name`, and `gameID` values
//! Moddin needs to detect the install and match it against the
//! `gogAppId` field of a catalog entry.
//!
//! GOG Galaxy maintains a richer SQLite database, but it is not always
//! installed (many users run standalone installers) and the SQL
//! dependency would balloon the binary. The registry path covers the
//! common case; the Galaxy path is left for a follow-up.

use crate::process::HideConsole;
use crate::steam::InstalledGame;
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    process::{Command, Stdio},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GogRegistryEntry {
    app_id: String,
    name: String,
    path: PathBuf,
}

const REGISTRY_ROOTS: &[&str] = &[
    r"HKLM\SOFTWARE\WOW6432Node\GOG.com\Games",
    r"HKLM\SOFTWARE\GOG.com\Games",
];

fn query_registry_subkeys(root: &str) -> Vec<String> {
    let output = Command::new("reg.exe")
        .hide_console()
        .args(["query", root])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();
    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    parse_subkeys_from_reg_output(&String::from_utf8_lossy(&output.stdout))
}

fn parse_subkeys_from_reg_output(stdout: &str) -> Vec<String> {
    // Reg output looks like
    //   HKEY_LOCAL_MACHINE\SOFTWARE\WOW6432Node\GOG.com\Games
    //   HKEY_LOCAL_MACHINE\SOFTWARE\WOW6432Node\GOG.com\Games\1234567890
    // We want only the trailing numeric segments so the parent key
    // (`Games`) is not surfaced as if it were a game id.
    stdout
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() || !trimmed.contains('\\') {
                return None;
            }
            let segment = trimmed.rsplit('\\').next()?.trim();
            if segment.is_empty() || !segment.chars().all(|c| c.is_ascii_digit()) {
                return None;
            }
            Some(segment.to_owned())
        })
        .collect()
}

fn read_gog_registry_value(root: &str, app_id: &str, value: &str) -> Option<String> {
    let output = Command::new("reg.exe")
        .hide_console()
        .args(["query", &format!("{}\\{}", root, app_id), "/v", value])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.lines().find_map(|line| {
        if !line.contains(value) {
            return None;
        }
        let marker_index = line.find("REG_SZ")?;
        let value = line[marker_index + "REG_SZ".len()..].trim();
        (!value.is_empty()).then(|| value.to_owned())
    })
}

fn read_entry(root: &str, app_id: &str) -> Option<GogRegistryEntry> {
    let name = read_gog_registry_value(root, app_id, "name")?;
    let path = read_gog_registry_value(root, app_id, "path")?;
    let path_buf = PathBuf::from(path);
    if !path_buf.is_dir() {
        return None;
    }
    Some(GogRegistryEntry {
        app_id: app_id.to_owned(),
        name,
        path: path_buf,
    })
}

fn detect_gog_installed_games_sync() -> Vec<InstalledGame> {
    let mut games = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for root in REGISTRY_ROOTS {
        for app_id in query_registry_subkeys(root) {
            // GOG game ids are decimal digits.
            if !app_id.chars().all(|character| character.is_ascii_digit()) {
                continue;
            }
            let Some(entry) = read_entry(root, &app_id) else {
                continue;
            };
            let key = format!("gog:{}", entry.app_id);
            if !seen.insert(key) {
                continue;
            }
            games.push(InstalledGame {
                store: "gog".to_owned(),
                app_id: entry.app_id,
                name: entry.name,
                install_dir: entry.path.to_string_lossy().into_owned(),
                library_path: entry.path.to_string_lossy().into_owned(),
            });
        }
    }
    games
}

#[tauri::command]
pub async fn detect_gog_installed_games() -> Result<Vec<InstalledGame>, String> {
    tauri::async_runtime::spawn_blocking(detect_gog_installed_games_sync)
        .await
        .map_err(|error| format!("GOG library scan task failed: {error}"))
}

/// Synchronous variant used by the background scanner, which already
/// runs the Steam/Epic detector on a blocking task and wants to append
/// GOG entries without an extra `Result` layer.
pub fn detect_gog_installed_games_blocking() -> Vec<InstalledGame> {
    detect_gog_installed_games_sync()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_registry_subkeys_from_expected_lines() {
        let stdout = "\
HKEY_LOCAL_MACHINE\\SOFTWARE\\WOW6432Node\\GOG.com\\Games

HKEY_LOCAL_MACHINE\\SOFTWARE\\WOW6432Node\\GOG.com\\Games\\1207658930
HKEY_LOCAL_MACHINE\\SOFTWARE\\WOW6432Node\\GOG.com\\Games\\1242989820
";
        let mut keys = parse_subkeys_from_reg_output(stdout);
        keys.sort();
        assert_eq!(
            keys,
            vec!["1207658930".to_owned(), "1242989820".to_owned()]
        );
    }

    #[test]
    fn rejects_non_digit_app_ids() {
        let stdout = "\
HKEY_LOCAL_MACHINE\\SOFTWARE\\WOW6432Node\\GOG.com\\Games\\abcd
HKEY_LOCAL_MACHINE\\SOFTWARE\\WOW6432Node\\GOG.com\\Games\\12345
";
        let keys: Vec<String> = stdout
            .lines()
            .filter_map(|line| {
                let trimmed = line.trim();
                if !trimmed.contains('\\') {
                    return None;
                }
                trimmed.rsplit('\\').next().map(str::to_owned)
            })
            .filter(|segment| segment.chars().all(|c| c.is_ascii_digit()))
            .collect();
        assert_eq!(keys, vec!["12345".to_owned()]);
    }
}
