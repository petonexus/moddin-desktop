//! Centralised path and identifier safety checks.
//!
//! Several modules had their own `safe_join_relative` and `is_valid_game_id`
//! implementations, each with the same shape and the same subtle
//! escape checks (no `..`, no absolute paths, no Windows drive
//! prefixes). This module is the single source of truth so future
//! modules and the upcoming `app/` reorg share the same definitions.
//!
//! Functions are intentionally free (not on a trait) because the
//! checks are stateless and have no useful polymorphism — the value
//! is DRY, not dynamic dispatch.

use std::{
    path::{Component, Path, PathBuf},
    string::String,
};

/// Returns `true` when the id contains only ASCII alphanumerics,
/// `-`, or `_`. Used by the per-game transaction store, ReShade
/// marker file naming, per-game OpenXR preference files, etc.
pub fn is_valid_game_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
}

/// Joins `relative` to `root` only when `relative` does not contain a
/// parent-directory, root, or path-prefix component. The original
/// implementation lived in `ofxr.rs`, `cheeky.rs`, `obs.rs`, and now
/// `reshade.rs` — this is the canonical helper.
pub fn safe_join_relative(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let relative_path = Path::new(relative);
    if relative_path.as_os_str().is_empty() {
        return Err("Catalog path is empty.".to_owned());
    }
    for component in relative_path.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(format!(
                    "Path escapes the allowed root: {relative}"
                ));
            }
        }
    }
    Ok(root.join(relative_path))
}

/// Validates that a candidate archive member name (a single path
/// segment inside a zip or tarball) is safe to extract under the
/// target root. Rejects `..`, absolute paths, Windows drive prefixes,
/// and empty names.
pub fn sanitize_archive_member(relative: &str) -> Option<String> {
    let path = Path::new(relative);
    let mut clean_segments: Vec<String> = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => {
                clean_segments.push(value.to_string_lossy().into_owned());
            }
            Component::CurDir => {}
            _ => return None,
        }
    }
    if clean_segments.is_empty() {
        None
    } else {
        Some(clean_segments.join("/"))
    }
}

/// Returns the executable name (`file_name`) of the path resulting
/// from `safe_join_relative(root, executable)`. Used by every module
/// that needs to call `crate::process::is_process_running` with a
/// process image name.
pub fn executable_process_name(
    root: &Path,
    executable: &str,
) -> Result<String, String> {
    let executable_path = safe_join_relative(root, executable)?;
    let name = executable_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "Could not resolve the game executable name.".to_owned())?;
    Ok(name.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_game_id_accepts_common_shapes() {
        assert!(is_valid_game_id("elden-ring"));
        assert!(is_valid_game_id("stalker_2"));
        assert!(is_valid_game_id("Cyberpunk2077"));
        assert!(!is_valid_game_id(""));
        assert!(!is_valid_game_id("../escape"));
        assert!(!is_valid_game_id("with space"));
        assert!(!is_valid_game_id("with/slash"));
    }

    #[test]
    fn safe_join_relative_rejects_parent_components() {
        let root = Path::new("C:/Games/Test");
        assert!(safe_join_relative(root, "../escape.exe").is_err());
        assert!(safe_join_relative(root, "sub/../escape.exe").is_err());
        assert!(safe_join_relative(root, "C:/Windows/notepad.exe").is_err());
    }

    #[test]
    fn safe_join_relative_accepts_normal_paths() {
        let root = Path::new("C:/Games/Test");
        assert_eq!(
            safe_join_relative(root, "Game/test.exe").unwrap(),
            PathBuf::from("C:/Games/Test/Game/test.exe")
        );
    }

    #[test]
    fn sanitize_archive_member_strips_traversal() {
        assert_eq!(sanitize_archive_member("ReShade64.dll").as_deref(), Some("ReShade64.dll"));
        assert_eq!(
            sanitize_archive_member("./ReShade/ReShade64.dll").as_deref(),
            Some("ReShade/ReShade64.dll")
        );
        assert_eq!(sanitize_archive_member("../escape.dll"), None);
        assert_eq!(sanitize_archive_member(""), None);
        assert_eq!(sanitize_archive_member("a/../../etc/passwd"), None);
    }
}