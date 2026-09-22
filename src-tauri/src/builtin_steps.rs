//! Built-in step executors.
//!
//! Every step kind declared by a [`crate::capability::StepSpec`] is
//! dispatched here. New kinds are added by extending
//! [`execute_step`] with a new match arm and updating
//! [`known_kinds`].
//!
//! Steps mutate state (filesystem, registry, processes) so they run
//! inside the transaction store: each successful install records a
//! [`crate::transaction::TransactionRecord`] the user can later undo.

use crate::{
    capability::{CapabilitySpec, ResolvedConfig, StepSpec},
    path_guard::sanitize_archive_member,
    transaction,
};
use serde_json::{json, Value as JsonValue};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{Cursor, Read, Write as IoWrite},
    path::{Path, PathBuf},
};
use zip::ZipArchive;

/// Catalogue of step kinds the runner knows how to execute. The
/// capability loader rejects specs that reference a kind outside this
/// set so the failure surfaces at startup.
pub fn known_kinds() -> &'static [&'static str] {
    &[
        "extract-zip",
        "verify-hash",
        "file-delete",
        "write-text-file",
        "spawn-process",
    ]
}

/// Output of a single step.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StepResult {
    pub kind: String,
    pub description: Option<String>,
    /// Files created, modified, or removed by this step. The runner
    /// accumulates these across all steps of an install so the
    /// transaction store knows what to back up / restore.
    pub affected_paths: Vec<String>,
}

/// Context the runner passes to each step.
pub struct StepContext<'a> {
    pub spec: &'a CapabilitySpec,
    pub config: &'a ResolvedConfig,
    pub install_directory: &'a Path,
    pub executable_directory: &'a Path,
}

/// Execute a single step. Returns the structured result or an error
/// string the UI surfaces verbatim.
pub fn execute_step(
    step: &StepSpec,
    context: &StepContext<'_>,
) -> Result<StepResult, String> {
    match step.kind.as_str() {
        "extract-zip" => run_extract_zip(step, context),
        "verify-hash" => run_verify_hash(step, context),
        "file-delete" => run_file_delete(step, context),
        "write-text-file" => run_write_text_file(step, context),
        "spawn-process" => run_spawn_process(step, context),
        other => Err(format!("Unknown step kind: {other}")),
    }
}

fn param<'a>(step: &'a StepSpec, name: &str) -> Option<&'a JsonValue> {
    step.params.get(name)
}

fn param_string<'a>(step: &'a StepSpec, name: &str) -> Option<&'a str> {
    step.params.get(name).and_then(|value| value.as_str())
}

fn run_extract_zip(
    step: &StepSpec,
    context: &StepContext<'_>,
) -> Result<StepResult, String> {
    // Two layouts supported:
    //   1. archiveBytesField + targetSubdir  — bytes come from a
    //      previously downloaded buffer held on the spec.
    //   2. archivePathField + targetSubdir    — bytes come from a path
    //      the runner resolves via config (local archive).
    //
    // Both extract every member of the archive into the install
    // directory after sanitising the member path. Members that match
    // a known proxy DLL filename are renamed to honour the recipe's
    // chosen proxy name (only when `proxyField` resolves to a non-empty
    // string).
    let bytes: Vec<u8> = if let Some(field) = param_string(step, "archiveBytesField") {
        return Err(format!(
            "extract-zip: archiveBytesField '{field}' requires the runner to \
             receive bytes from a previous download step. Not yet implemented; \
             use archivePathField pointing at a local file for now."
        ));
    } else if let Some(field) = param_string(step, "archivePathField") {
        let path = context
            .config
            .get_string(field)
            .ok_or_else(|| format!("extract-zip: config field '{field}' is missing."))?;
        fs::read(&path).map_err(|error| {
            format!("extract-zip: could not read archive '{path}': {error}")
        })?
    } else {
        return Err(
            "extract-zip: step needs archiveBytesField or archivePathField.".to_owned(),
        );
    };

    let mut archive = ZipArchive::new(Cursor::new(bytes.as_slice()))
        .map_err(|error| format!("extract-zip: could not open zip: {error}"))?;

    fs::create_dir_all(context.executable_directory)
        .map_err(|error| format!("extract-zip: could not create target directory: {error}"))?;

    let proxy = param_string(step, "proxyField")
        .and_then(|field| context.config.get_string(field));

    let mut affected = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("extract-zip: entry {index} unreadable: {error}"))?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_owned();
        let Some(safe_name) = sanitize_archive_member(&name) else {
            return Err(format!("extract-zip: unsafe archive member '{name}'."));
        };
        let basename = Path::new(&safe_name)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();

        let target = if let Some(proxy_name) = proxy.as_ref() {
            if basename == "reshade64.dll" || basename == "dxgi.dll" {
                let candidate = context.executable_directory.join(proxy_name);
                affected.push(proxy_name.clone());
                candidate
            } else {
                let candidate = context.executable_directory.join(&safe_name);
                affected.push(safe_name.clone());
                candidate
            }
        } else {
            let candidate = context.executable_directory.join(&safe_name);
            affected.push(safe_name.clone());
            candidate
        };

        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!("extract-zip: could not create parent dir: {error}")
            })?;
        }
        let mut buffer = Vec::new();
        entry
            .read_to_end(&mut buffer)
            .map_err(|error| format!("extract-zip: could not read '{name}': {error}"))?;
        let mut file = File::create(&target)
            .map_err(|error| format!("extract-zip: could not create target: {error}"))?;
        file.write_all(&buffer)
            .map_err(|error| format!("extract-zip: could not write target: {error}"))?;
    }

    Ok(StepResult {
        kind: step.kind.clone(),
        description: step.description.clone(),
        affected_paths: affected,
    })
}

fn run_verify_hash(
    step: &StepSpec,
    context: &StepContext<'_>,
) -> Result<StepResult, String> {
    let path_field = param_string(step, "pathField")
        .ok_or_else(|| "verify-hash: pathField is required.".to_owned())?;
    let path = context
        .config
        .get_string(path_field)
        .ok_or_else(|| format!("verify-hash: config field '{path_field}' is missing."))?;
    let expected = param_string(step, "expectedField")
        .and_then(|field| context.config.get_string(field))
        .or_else(|| param_string(step, "expected").map(str::to_owned))
        .ok_or_else(|| "verify-hash: expectedField or expected is required.".to_owned())?;

    let mut file = File::open(&path)
        .map_err(|error| format!("verify-hash: could not open '{path}': {error}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("verify-hash: read error: {error}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let computed = format!("{:x}", hasher.finalize());
    if !computed.eq_ignore_ascii_case(&expected) {
        return Err(format!(
            "verify-hash: SHA-256 mismatch for '{path}' (expected {expected}, got {computed})."
        ));
    }

    Ok(StepResult {
        kind: step.kind.clone(),
        description: step.description.clone(),
        affected_paths: Vec::new(),
    })
}

fn run_file_delete(
    step: &StepSpec,
    context: &StepContext<'_>,
) -> Result<StepResult, String> {
    let path_field = param_string(step, "pathField")
        .ok_or_else(|| "file-delete: pathField is required.".to_owned())?;
    let path = context
        .config
        .get_string(path_field)
        .ok_or_else(|| format!("file-delete: config field '{path_field}' is missing."))?;
    let resolved = resolve_path(context.executable_directory, &path);
    if !resolved.is_file() {
        return Err(format!("file-delete: '{path}' is not a regular file."));
    }
    fs::remove_file(&resolved)
        .map_err(|error| format!("file-delete: could not remove '{path}': {error}"))?;
    Ok(StepResult {
        kind: step.kind.clone(),
        description: step.description.clone(),
        affected_paths: vec![resolved.to_string_lossy().into_owned()],
    })
}

fn run_write_text_file(
    step: &StepSpec,
    context: &StepContext<'_>,
) -> Result<StepResult, String> {
    let path_field = param_string(step, "pathField")
        .ok_or_else(|| "write-text-file: pathField is required.".to_owned())?;
    let path = context
        .config
        .get_string(path_field)
        .ok_or_else(|| format!("write-text-file: config field '{path_field}' is missing."))?;
    let resolved = resolve_path(context.executable_directory, &path);
    if let Some(parent) = resolved.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!("write-text-file: could not create parent dir: {error}")
        })?;
    }
    let template = param(step, "template")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let rendered = render_template(template, context.config);
    fs::write(&resolved, rendered)
        .map_err(|error| format!("write-text-file: could not write '{path}': {error}"))?;
    Ok(StepResult {
        kind: step.kind.clone(),
        description: step.description.clone(),
        affected_paths: vec![resolved.to_string_lossy().into_owned()],
    })
}

fn run_spawn_process(
    step: &StepSpec,
    context: &StepContext<'_>,
) -> Result<StepResult, String> {
    let executable = param_string(step, "executable")
        .ok_or_else(|| "spawn-process: executable is required.".to_owned())?;
    let resolved = resolve_path(context.executable_directory, executable);
    if !resolved.is_file() {
        return Err(format!(
            "spawn-process: '{executable}' is not a regular file at {}.",
            resolved.display()
        ));
    }
    let mut command = std::process::Command::new(&resolved);
    if let Some(parent) = resolved.parent() {
        command.current_dir(parent);
    }
    let child = command
        .spawn()
        .map_err(|error| format!("spawn-process: could not start '{executable}': {error}"))?;
    Ok(StepResult {
        kind: step.kind.clone(),
        description: step.description.clone(),
        affected_paths: vec![resolved.to_string_lossy().into_owned()],
        // Keep the child PID handy for callers via the runtime;
        // today we just drop it — Moddin tracks tray liveness through
        // process snapshots instead of carrying child handles.
    })
    .map(|mut result| {
        result.affected_paths.push(format!("pid:{}", child.id()));
        result
    })
}

fn render_template(template: &str, config: &ResolvedConfig) -> String {
    let mut output = String::with_capacity(template.len());
    let mut chars = template.chars().peekable();
    while let Some(character) = chars.next() {
        if character == '{' && chars.peek() == Some(&'}') {
            chars.next();
            // Empty placeholder — leave as is.
            output.push_str("{}");
            continue;
        }
        if character == '{' {
            let mut name = String::new();
            for next in chars.by_ref() {
                if next == '}' {
                    break;
                }
                name.push(next);
            }
            if let Some(value) = config.get(&name).and_then(|v| -> Option<String> {
                match v {
                    JsonValue::String(string) => Some(string.clone()),
                    JsonValue::Bool(boolean) => {
                        Some(if *boolean { "1".to_string() } else { "0".to_string() })
                    }
                    JsonValue::Number(number) => number.as_u64().map(|v| v.to_string()),
                    _ => None,
                }
            }) {
                output.push_str(&value);
            } else {
                output.push('{');
                output.push_str(&name);
                output.push('}');
            }
            continue;
        }
        output.push(character);
    }
    output
}

fn resolve_path(base: &Path, candidate: &str) -> PathBuf {
    let path = Path::new(candidate);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}

/// Convenience helper used by tests and by external callers that
/// want to record the install transaction without re-implementing the
/// orchestration loop.
pub fn record_install_transaction(
    spec: &CapabilitySpec,
    game_id: &str,
    game_name: &str,
    install_directory: &Path,
    affected: &[String],
    metadata: BTreeMap<String, String>,
) -> Result<transaction::TransactionRecord, String> {
    let targets: Vec<PathBuf> = affected.iter().map(PathBuf::from).collect();
    let record = transaction::begin_file_set_transaction(
        install_directory,
        &targets,
        &spec.id,
        &format!("Install {} for {}", spec.display_name, game_name),
        game_id,
        metadata,
    )?;
    transaction::mark_applied(record)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_kinds_include_the_core_set() {
        let kinds = known_kinds();
        assert!(kinds.contains(&"extract-zip"));
        assert!(kinds.contains(&"verify-hash"));
        assert!(kinds.contains(&"file-delete"));
        assert!(kinds.contains(&"write-text-file"));
        assert!(kinds.contains(&"spawn-process"));
    }

    #[test]
    fn template_renders_known_placeholders() {
        let mut config = ResolvedConfig::default();
        config.values.insert("backend".to_owned(), json!("fidelityfx"));
        config
            .values
            .insert("nvidia_preset".to_owned(), json!("medium"));
        let rendered = render_template("[tray]\nbackend={backend}\nnvidia_preset={nvidia_preset}\n", &config);
        assert_eq!(rendered, "[tray]\nbackend=fidelityfx\nnvidia_preset=medium\n");
    }

    #[test]
    fn template_leaves_unknown_placeholders_intact() {
        let config = ResolvedConfig::default();
        let rendered = render_template("x={missing}", &config);
        assert_eq!(rendered, "x={missing}");
    }
}