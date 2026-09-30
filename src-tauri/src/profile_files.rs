//! Reading and writing exported Moddin profile files.
//!
//! A profile is a plain JSON document the user keeps a copy of and may
//! mail to themselves, so the only filesystem work the feature needs is
//! "put this document somewhere I can find it" and "read a document back
//! from a path the user names". Both live here rather than in the UI
//! because this app's WebView has no filesystem or file-dialog permission
//! of its own: the Tauri capability set is `core:default` and neither
//! `tauri-plugin-fs` nor `tauri-plugin-dialog` is wired in, so a
//! `<input type="file">` or an anchor download is the only thing the
//! frontend could reach — neither is a save dialog, and neither is a path
//! the app can hand back to a later install.
//!
//! Exports are written to `%LOCALAPPDATA%\Moddin\profiles\exports\`,
//! the same shape as the transaction store and the per-game OpenXR
//! override. Reads accept any absolute path, because the whole point of
//! the feature is a file that has travelled somewhere else first.
//!
//! Nothing here parses or validates a profile. The schema version check
//! and the secrets rule live in the frontend, next to the copy that
//! explains them, so there is exactly one definition of each.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// A profile is a list of capability ids and their config values. A file
/// bigger than this is not one, and reading it into the WebView is the
/// expensive half of the operation.
const MAX_PROFILE_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteProfileFileRequest {
    /// A bare file name, never a path. The folder is Moddin's.
    pub file_name: String,
    pub contents: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProfileFileResult {
    pub path: String,
    pub bytes: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadProfileFileRequest {
    pub path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileFileContents {
    pub path: String,
    pub contents: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RevealProfileFileRequest {
    pub path: String,
}

/// `%LOCALAPPDATA%\Moddin\profiles\exports\`, falling back to the temp
/// directory on a non-Windows dev machine, exactly like the transaction
/// store and the capability override directory do.
fn exports_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("Moddin")
        .join("profiles")
        .join("exports")
}

/// A profile export is a bare `*.json` file name. `sanitize_archive_member`
/// is the repo's single source of truth for "no `..`, no absolute path, no
/// drive prefix", and the two extra rules are that a name may not contain
/// a separator (it is one file, not a tree) and has to end in `.json`, so
/// the user can double-click it later and know what they are opening.
fn sanitize_export_name(file_name: &str) -> Result<String, String> {
    let Some(name) = crate::path_guard::sanitize_archive_member(file_name.trim()) else {
        return Err("A profile file name cannot contain a folder or '..'.".to_owned());
    };
    if name.contains('/') || name.contains('\\') {
        return Err("A profile file name cannot contain a folder.".to_owned());
    }
    if !name.to_ascii_lowercase().ends_with(".json") {
        return Err("A profile file has to end in .json.".to_owned());
    }
    Ok(name)
}

/// Extracted from the command so the tests can point it at a temp
/// directory instead of the user's real profile folder.
fn write_export_to_dir(
    dir: &Path,
    file_name: &str,
    contents: &str,
) -> Result<ProfileFileResult, String> {
    let name = sanitize_export_name(file_name)?;
    if contents.len() as u64 > MAX_PROFILE_BYTES {
        return Err("This profile is too large to be a Moddin profile.".to_owned());
    }
    std::fs::create_dir_all(dir)
        .map_err(|error| format!("Could not create the Moddin profiles folder: {error}"))?;
    let path = dir.join(&name);
    // Overwriting is the expected behaviour when the user re-exports
    // under the same name; the folder only ever holds profiles this app
    // wrote, and the previous copy is the user's to keep elsewhere.
    std::fs::write(&path, contents)
        .map_err(|error| format!("Could not write the profile file: {error}"))?;
    Ok(ProfileFileResult {
        path: path.to_string_lossy().into_owned(),
        bytes: contents.len(),
    })
}

fn read_profile_at(path: &Path) -> Result<ProfileFileContents, String> {
    if !path.is_absolute() {
        return Err("A profile path has to be absolute.".to_owned());
    }
    let metadata =
        std::fs::metadata(path).map_err(|error| format!("Could not open that file: {error}"))?;
    if !metadata.is_file() {
        return Err("That path is a folder, not a profile file.".to_owned());
    }
    if metadata.len() > MAX_PROFILE_BYTES {
        return Err("That file is too large to be a Moddin profile.".to_owned());
    }
    let contents = std::fs::read_to_string(path)
        .map_err(|error| format!("That file could not be read as text: {error}"))?;
    Ok(ProfileFileContents {
        path: path.to_string_lossy().into_owned(),
        contents,
    })
}

#[tauri::command]
pub fn write_profile_file(request: WriteProfileFileRequest) -> Result<ProfileFileResult, String> {
    write_export_to_dir(&exports_dir(), &request.file_name, &request.contents)
}

#[tauri::command]
pub fn read_profile_file(request: ReadProfileFileRequest) -> Result<ProfileFileContents, String> {
    read_profile_at(Path::new(request.path.trim()))
}

/// Show the exported file in the system file browser.
///
/// There is no save dialog in this build, so "where did it go?" has to
/// have an answer that does not require the user to type a path into
/// Explorer. The platform branch mirrors `open_url_in_browser` in
/// `lib.rs` rather than introducing a new convention.
#[tauri::command]
pub fn reveal_profile_file(request: RevealProfileFileRequest) -> Result<(), String> {
    let path = Path::new(request.path.trim());
    if !path.is_absolute() {
        return Err("A profile path has to be absolute.".to_owned());
    }
    if !path.is_file() {
        return Err("That profile file is not there any more.".to_owned());
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer.exe")
            .arg(format!("/select,{}", path.display()))
            .spawn()
            .map(|_| ())
            .map_err(|error| format!("Could not open the folder: {error}"))
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-R")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|error| format!("Could not open the folder: {error}"))
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let folder = path.parent().unwrap_or(path);
        std::process::Command::new("xdg-open")
            .arg(folder)
            .spawn()
            .map(|_| ())
            .map_err(|error| format!("Could not open the folder: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("moddin-profile-files-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn export_names_are_bare_json_files() {
        assert_eq!(
            sanitize_export_name("elden-ring.json").unwrap(),
            "elden-ring.json"
        );
        assert_eq!(
            sanitize_export_name("  elden-ring.json  ").unwrap(),
            "elden-ring.json"
        );
        // Case is left alone; only the extension is matched case-insensitively.
        assert_eq!(
            sanitize_export_name("Profile.JSON").unwrap(),
            "Profile.JSON"
        );
    }

    #[test]
    fn export_names_reject_traversal_folders_and_other_extensions() {
        assert!(sanitize_export_name("../escape.json").is_err());
        assert!(sanitize_export_name("..\\escape.json").is_err());
        assert!(sanitize_export_name("C:/Windows/escape.json").is_err());
        assert!(sanitize_export_name("nested/profile.json").is_err());
        assert!(sanitize_export_name("nested\\profile.json").is_err());
        assert!(sanitize_export_name("profile.yaml").is_err());
        assert!(sanitize_export_name("").is_err());
    }

    #[test]
    fn a_written_export_reads_back_byte_for_byte() {
        let dir = temp_dir("roundtrip");
        let contents = r#"{"kind":"moddin-profile","schemaVersion":1}"#;

        let written = write_export_to_dir(&dir, "profile.json", contents).unwrap();
        assert_eq!(written.bytes, contents.len());
        assert!(written.path.ends_with("profile.json"));

        let read = read_profile_at(Path::new(&written.path)).unwrap();
        assert_eq!(read.contents, contents);
        assert_eq!(read.path, written.path);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_traversal_name_never_reaches_the_filesystem() {
        let dir = temp_dir("traversal");
        let error = write_export_to_dir(&dir, "../escape.json", "{}").unwrap_err();
        assert!(error.contains("folder"));
        assert!(!dir.parent().unwrap().join("escape.json").exists());
        assert!(!dir.exists());
    }

    #[test]
    fn writing_the_same_name_twice_replaces_the_file() {
        let dir = temp_dir("overwrite");
        write_export_to_dir(&dir, "profile.json", "first").unwrap();
        let second = write_export_to_dir(&dir, "profile.json", "second").unwrap();
        let read = read_profile_at(Path::new(&second.path)).unwrap();
        assert_eq!(read.contents, "second");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reading_refuses_relative_paths_missing_files_and_folders() {
        let dir = temp_dir("read-refusals");
        std::fs::create_dir_all(&dir).unwrap();

        assert!(read_profile_at(Path::new("profile.json"))
            .unwrap_err()
            .contains("absolute"));
        assert!(read_profile_at(&dir.join("missing.json")).is_err());
        assert!(read_profile_at(&dir).unwrap_err().contains("folder"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_oversized_document_is_refused_before_it_is_written() {
        let dir = temp_dir("too-large");
        let contents = "x".repeat((MAX_PROFILE_BYTES + 1) as usize);

        let error = write_export_to_dir(&dir, "profile.json", &contents).unwrap_err();
        assert!(error.contains("too large"));
        assert!(!dir.exists());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
