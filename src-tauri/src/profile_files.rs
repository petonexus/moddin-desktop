//! Reading and writing exported Moddin profile files.
//!
//! A profile is a plain JSON document the user keeps a copy of and may
//! mail to themselves, so the only filesystem work the feature needs is
//! "put this document where the user says" and "read a document back
//! from a path the user names". Both live here rather than in the UI
//! because this app's WebView has no filesystem or file-dialog permission
//! of its own: the Tauri capability set is `core:default`, and
//! `src-tauri/capabilities/dialog.json` deliberately grants the webview
//! nothing from `tauri-plugin-dialog`. The dialogs are opened from the
//! commands below, so the only thing that can write a profile is a
//! command that has just been told which path the user picked.
//!
//! Two rules the whole file is built around:
//!
//!  - **A closed dialog is not a failure.** Both commands answer `None`
//!    when the user dismisses the window, which the view reads as
//!    "nothing happened". A refusal the user can do nothing about is a
//!    worse bug than a file in the wrong place would be.
//!  - **Nothing is written that the user did not name.** There is no
//!    default destination and no folder of Moddin's own left in here: an
//!    export goes where the Save-As dialog was pointed, or nowhere.
//!
//! Reads accept any absolute path, because the whole point of the
//! feature is a file that has travelled somewhere else first.
//!
//! Nothing here parses or validates a profile. The schema version check
//! and the secrets rule live in the frontend, next to the copy that
//! explains them, so there is exactly one definition of each.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::AppHandle;
use tauri_plugin_dialog::{DialogExt, FilePath};

/// A profile is a list of capability ids and their config values. A file
/// bigger than this is not one, and reading it into the WebView is the
/// expensive half of the operation.
const MAX_PROFILE_BYTES: u64 = 2 * 1024 * 1024;

/// The one file type either dialog offers. The extension is enforced
/// again in `validate_destination`, because a filter is a suggestion to
/// the shell and a guarantee to us.
const PROFILE_FILTER: &str = "Moddin profile";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveProfileFileRequest {
    /// The name the dialog starts with. Prefilled text, not a
    /// destination: where the file goes is the user's answer.
    pub file_name: String,
    pub contents: String,
}

/// Mirrored by `ProfileFileResult` on the frontend. The command answers
/// `None` instead when the dialog was dismissed.
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

/// The name the Save-As dialog starts with.
///
/// This is prefilled text in a text box, not a destination, so the rules
/// are the strict ones anyway: `sanitize_archive_member` is the repo's
/// single source of truth for "no `..`, no absolute path, no drive
/// prefix", and on top of that a default may not contain a separator (it
/// is one file, not a tree) and has to end in `.json`, so the user can
/// double-click the result later and know what they are opening.
fn dialog_default_name(file_name: &str) -> Result<String, String> {
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

/// Everything that has to be true of a destination the user chose.
///
/// A save dialog returns a path; it does not promise the path is a file
/// this app should write. A folder, a name the shell left without the
/// `.json` extension, or a folder that has since been deleted are all
/// refused here rather than turned into a confusing error from
/// `std::fs::write` — or, worse, into a file at a name the user did not
/// ask for.
fn validate_destination(path: &Path) -> Result<(), String> {
    if !path.is_absolute() {
        return Err("A profile has to be saved to a full path.".to_owned());
    }
    let Some(name) = path.file_name() else {
        return Err("That is a folder, not a profile file.".to_owned());
    };
    if name.is_empty() {
        return Err("That is a folder, not a profile file.".to_owned());
    }
    if !name
        .to_string_lossy()
        .to_ascii_lowercase()
        .ends_with(".json")
    {
        return Err("A profile file has to end in .json.".to_owned());
    }
    match path.parent() {
        Some(parent) if parent.is_dir() => Ok(()),
        _ => Err("That folder is not there any more.".to_owned()),
    }
}

/// Refuse a document that could never be written, before a dialog is
/// opened to ask the user to choose a place for it.
fn check_export_size(contents: &str) -> Result<(), String> {
    if contents.len() as u64 > MAX_PROFILE_BYTES {
        return Err("This profile is too large to be a Moddin profile.".to_owned());
    }
    Ok(())
}

/// What the command does with the dialog's answer.
///
/// Split out from the command so the one case that must never be wrong
/// — the user closed the window — is a test rather than a hope: `None`
/// is `Ok(None)`, and the function returns before it has named a path,
/// let alone touched the disk.
fn finish_save(
    chosen: Option<PathBuf>,
    contents: &str,
) -> Result<Option<ProfileFileResult>, String> {
    let Some(path) = chosen else {
        return Ok(None);
    };
    validate_destination(&path)?;
    // Overwriting is the expected behaviour when the user saves over a
    // name they chose twice; the shell has already asked them to
    // confirm it, and the previous copy is theirs to keep elsewhere.
    std::fs::write(&path, contents)
        .map_err(|error| format!("Could not write the profile file: {error}"))?;
    Ok(Some(ProfileFileResult {
        path: path.to_string_lossy().into_owned(),
        bytes: contents.len(),
    }))
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

/// Ask the user where the profile goes, then write it there.
///
/// The dialog is opened from here rather than from the webview because
/// the webview holds no permission for the plugin
/// (`src-tauri/capabilities/dialog.json`), and that is the point: there
/// is no command that writes a profile to a path the webview chose.
///
/// The answer is `None` when the user closed the dialog, and the file is
/// then not written. That is the whole contract: "no" is not a failure,
/// and the panel shows nothing at all in that case.
#[tauri::command]
pub async fn save_profile_file(
    app: AppHandle,
    request: SaveProfileFileRequest,
) -> Result<Option<ProfileFileResult>, String> {
    // Both refusals happen before the window opens: there is no sense
    // asking a user to choose a destination for a file this app will
    // not write.
    let default_name = dialog_default_name(&request.file_name)?;
    check_export_size(&request.contents)?;

    // The callback form, not `blocking_save_file`: this command is
    // awaited on the async runtime and blocking a thread on a modal
    // window is the way the plugin documents as freezing the app.
    let (sender, mut receiver) = tauri::async_runtime::channel::<Option<FilePath>>(1);
    app.dialog()
        .file()
        .set_title("Save your Moddin profile")
        .add_filter(PROFILE_FILTER, &["json"])
        .set_file_name(default_name)
        .save_file(move |chosen| {
            // The send has to be *polled* to enqueue anything, and
            // `let _ = sender.send(chosen)` drops that future unpolled —
            // clippy names it, and it is right to. The value would never
            // reach the channel, `recv()` would only return once the
            // sender is dropped with the closure, and every save would
            // report "the dialog closed without an answer".
            tauri::async_runtime::spawn(async move {
                let _ = sender.send(chosen).await;
            });
        });

    let chosen = receiver
        .recv()
        .await
        .ok_or_else(|| "The save dialog closed without an answer.".to_owned())?;
    let Some(chosen) = chosen else {
        return Ok(None);
    };
    let path = chosen
        .into_path()
        .map_err(|error| format!("The save dialog did not return a file path: {error}"))?;

    finish_save(Some(path), &request.contents)
}

/// Ask which profile file to read.
///
/// `None` means the user closed the dialog. The command answers with a
/// path and nothing else — the file is read by `read_profile_file`,
/// which is the same command the path box has always used, so a file
/// chosen here and a path typed there are judged by one set of rules.
#[tauri::command]
pub async fn pick_profile_file(app: AppHandle) -> Result<Option<String>, String> {
    let (sender, mut receiver) = tauri::async_runtime::channel::<Option<FilePath>>(1);
    app.dialog()
        .file()
        .set_title("Choose a Moddin profile")
        .add_filter(PROFILE_FILTER, &["json"])
        .pick_file(move |chosen| {
            // Spawned, not dropped: see the save dialog above. A channel
            // send is a future, and an unpolled future enqueues nothing.
            tauri::async_runtime::spawn(async move {
                let _ = sender.send(chosen).await;
            });
        });

    let chosen = receiver
        .recv()
        .await
        .ok_or_else(|| "The open dialog closed without an answer.".to_owned())?;
    let Some(chosen) = chosen else {
        return Ok(None);
    };
    let path = chosen
        .into_path()
        .map_err(|error| format!("The open dialog did not return a file path: {error}"))?;
    if !path.is_absolute() {
        return Err("A profile path has to be absolute.".to_owned());
    }
    Ok(Some(path.to_string_lossy().into_owned()))
}

#[tauri::command]
pub fn read_profile_file(request: ReadProfileFileRequest) -> Result<ProfileFileContents, String> {
    read_profile_at(Path::new(request.path.trim()))
}

/// Show the exported file in the system file browser.
///
/// Kept because a profile is a file the user has just sent somewhere,
/// and "which of those was it?" is a question the panel should answer
/// without making them remember the folder. The platform branch mirrors
/// `open_url_in_browser` in `lib.rs` rather than introducing a new
/// convention.
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
    fn dialog_default_names_are_bare_json_files() {
        assert_eq!(
            dialog_default_name("elden-ring.json").unwrap(),
            "elden-ring.json"
        );
        assert_eq!(
            dialog_default_name("  elden-ring.json  ").unwrap(),
            "elden-ring.json"
        );
        // Case is left alone; only the extension is matched case-insensitively.
        assert_eq!(dialog_default_name("Profile.JSON").unwrap(), "Profile.JSON");
    }

    #[test]
    fn a_cancelled_save_dialog_writes_nothing() {
        let dir = temp_dir("cancelled");

        let result = finish_save(None, "{}").unwrap();

        assert_eq!(result, None);
        assert!(!dir.exists());
    }

    #[test]
    fn a_chosen_destination_is_written_and_reads_back_byte_for_byte() {
        let dir = temp_dir("chosen");
        std::fs::create_dir_all(&dir).unwrap();
        let chosen = dir.join("profile.json");
        let contents = r#"{"kind":"moddin-profile","schemaVersion":1}"#;

        let result = finish_save(Some(chosen.clone()), contents)
            .unwrap()
            .unwrap();

        assert_eq!(result.path, chosen.to_string_lossy());
        assert_eq!(result.bytes, contents.len());
        assert_eq!(read_profile_at(&chosen).unwrap().contents, contents);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_destination_that_is_not_a_json_file_in_an_existing_folder_is_refused() {
        let dir = temp_dir("destination");
        std::fs::create_dir_all(&dir).unwrap();

        // A relative path, a name without the extension, and a folder
        // that is not there are all refused before anything is written.
        assert!(validate_destination(Path::new("profile.json"))
            .unwrap_err()
            .contains("full path"));
        assert!(validate_destination(&dir.join("profile.txt"))
            .unwrap_err()
            .contains(".json"));
        assert!(
            validate_destination(&dir.join("nested").join("profile.json"))
                .unwrap_err()
                .contains("folder")
        );
        assert!(validate_destination(&dir).is_err());
        assert_eq!(
            std::fs::read_dir(&dir).unwrap().count(),
            0,
            "a refused destination must leave the disk alone"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dialog_default_names_reject_traversal_folders_and_other_extensions() {
        assert!(dialog_default_name("../escape.json").is_err());
        assert!(dialog_default_name("..\\escape.json").is_err());
        assert!(dialog_default_name("C:/Windows/escape.json").is_err());
        assert!(dialog_default_name("nested/profile.json").is_err());
        assert!(dialog_default_name("nested\\profile.json").is_err());
        assert!(dialog_default_name("profile.yaml").is_err());
        assert!(dialog_default_name("").is_err());
    }

    #[test]
    fn a_refused_default_name_never_reaches_a_dialog() {
        // The default is text in a text box, so this cannot write
        // anything — but it must not become a directory the save dialog
        // starts inside either.
        let error = dialog_default_name("..\\escape.json").unwrap_err();
        assert!(error.contains("folder"));
    }

    #[test]
    fn saving_to_the_same_chosen_name_twice_replaces_the_file() {
        let dir = temp_dir("overwrite");
        std::fs::create_dir_all(&dir).unwrap();
        let chosen = dir.join("profile.json");

        finish_save(Some(chosen.clone()), "first").unwrap();
        let second = finish_save(Some(chosen.clone()), "second")
            .unwrap()
            .unwrap();

        assert_eq!(second.path, chosen.to_string_lossy());
        assert_eq!(read_profile_at(&chosen).unwrap().contents, "second");

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
    fn an_oversized_document_is_refused_before_the_dialog_opens() {
        let contents = "x".repeat((MAX_PROFILE_BYTES + 1) as usize);

        let error = check_export_size(&contents).unwrap_err();
        assert!(error.contains("too large"));
        assert!(check_export_size("{}").is_ok());
    }
}
