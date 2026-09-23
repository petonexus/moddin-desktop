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
        "write-binary-file",
        "move-file",
        "kill-process",
        "registry-write",
        "registry-delete",
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
        "write-binary-file" => run_write_binary_file(step, context),
        "move-file" => run_move_file(step, context),
        "kill-process" => run_kill_process(step, context),
        "registry-write" => run_registry_write(step, context),
        "registry-delete" => run_registry_delete(step, context),
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

fn run_write_binary_file(
    step: &StepSpec,
    context: &StepContext<'_>,
) -> Result<StepResult, String> {
    let path_field = param_string(step, "pathField")
        .ok_or_else(|| "write-binary-file: pathField is required.".to_owned())?;
    let path = context
        .config
        .get_string(path_field)
        .ok_or_else(|| format!("write-binary-file: config field '{path_field}' is missing."))?;
    let resolved = resolve_path(context.executable_directory, &path);
    if let Some(parent) = resolved.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!("write-binary-file: could not create parent dir: {error}")
        })?;
    }
    let bytes = param_string(step, "base64")
        .and_then(|value| base64_decode(value))
        .ok_or_else(|| "write-binary-file: base64 param with required hex.".to_owned())?;
    fs::write(&resolved, &bytes)
        .map_err(|error| format!("write-binary-file: could not write '{path}': {error}"))?;
    Ok(StepResult {
        kind: step.kind.clone(),
        description: step.description.clone(),
        affected_paths: vec![resolved.to_string_lossy().into_owned()],
    })
}

fn run_move_file(
    step: &StepSpec,
    context: &StepContext<'_>,
) -> Result<StepResult, String> {
    let from_field = param_string(step, "fromField")
        .ok_or_else(|| "move-file: fromField is required.".to_owned())?;
    let to_field = param_string(step, "toField")
        .ok_or_else(|| "move-file: toField is required.".to_owned())?;
    let from_path = context
        .config
        .get_string(from_field)
        .ok_or_else(|| format!("move-file: config field '{from_field}' is missing."))?;
    let to_path = context
        .config
        .get_string(to_field)
        .ok_or_else(|| format!("move-file: config field '{to_field}' is missing."))?;
    let from = resolve_path(context.executable_directory, &from_path);
    let to = resolve_path(context.executable_directory, &to_path);
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!("move-file: could not create destination parent: {error}")
        })?;
    }
    fs::rename(&from, &to)
        .map_err(|error| format!("move-file: could not move '{from_path}' -> '{to_path}': {error}"))?;
    Ok(StepResult {
        kind: step.kind.clone(),
        description: step.description.clone(),
        affected_paths: vec![
            from.to_string_lossy().into_owned(),
            to.to_string_lossy().into_owned(),
        ],
    })
}

fn run_kill_process(
    step: &StepSpec,
    _context: &StepContext<'_>,
) -> Result<StepResult, String> {
    let process_name = param_string(step, "processName")
        .or_else(|| param_string(step, "processNameField"))
        .ok_or_else(|| "kill-process: processName or processNameField is required.".to_owned())?;
    let force = param(step, "force")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let mut command = std::process::Command::new("taskkill");
    crate::process::HideConsole::hide_console(&mut command);
    command.arg("/IM").arg(process_name).arg("/T");
    if force {
        command.arg("/F");
    }
    let output = command
        .output()
        .map_err(|error| format!("kill-process: could not spawn taskkill: {error}"))?;
    let code = output.status.code().unwrap_or(-1);
    if code != 0 && code != 128 {
        // 128 == ERROR_NOT_FOUND, which is fine for an idempotent kill.
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "kill-process: taskkill exited {code}: {stderr}"
        ));
    }
    Ok(StepResult {
        kind: step.kind.clone(),
        description: step.description.clone(),
        affected_paths: Vec::new(),
    })
}

fn run_registry_write(
    step: &StepSpec,
    _context: &StepContext<'_>,
) -> Result<StepResult, String> {
    let key = param_string(step, "key")
        .ok_or_else(|| "registry-write: key is required.".to_owned())?;
    let value = param_string(step, "value")
        .ok_or_else(|| "registry-write: value is required.".to_owned())?;
    let value_kind = param_string(step, "type")
        .unwrap_or("REG_SZ");
    let force = param(step, "force")
        .and_then(|value| value.as_bool())
        .unwrap_or(true);

    let mut command = std::process::Command::new("reg.exe");
    crate::process::HideConsole::hide_console(&mut command);
    command.arg("add").arg(key);
    if value_kind == "REG_SZ" || value_kind == "REG_EXPAND_SZ" || value_kind == "REG_DWORD" {
        command.arg("/v").arg(value).arg("/t").arg(value_kind);
    } else if value_kind == "REG_BINARY" || value_kind == "REG_MULTI_SZ" {
        command.arg("/v").arg(value).arg("/t").arg(value_kind).arg("/d").arg("");
    } else {
        return Err(format!(
            "registry-write: unsupported value type '{value_kind}'."
        ));
    }
    if force {
        command.arg("/f");
    }
    let output = command
        .output()
        .map_err(|error| format!("registry-write: could not spawn reg.exe: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "registry-write: reg.exe failed ({}): {stderr}",
            output.status
        ));
    }
    Ok(StepResult {
        kind: step.kind.clone(),
        description: step.description.clone(),
        affected_paths: vec![format!("registry:{key}")],
    })
}

fn run_registry_delete(
    step: &StepSpec,
    _context: &StepContext<'_>,
) -> Result<StepResult, String> {
    let key = param_string(step, "key")
        .ok_or_else(|| "registry-delete: key is required.".to_owned())?;
    let value = param_string(step, "value");
    let force = param(step, "force")
        .and_then(|value| value.as_bool())
        .unwrap_or(true);

    let mut command = std::process::Command::new("reg.exe");
    crate::process::HideConsole::hide_console(&mut command);
    command.arg("delete").arg(key);
    if let Some(name) = value {
        command.arg("/v").arg(name);
    }
    if force {
        command.arg("/f");
    }
    let output = command
        .output()
        .map_err(|error| format!("registry-delete: could not spawn reg.exe: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "registry-delete: reg.exe failed ({}): {stderr}",
            output.status
        ));
    }
    Ok(StepResult {
        kind: step.kind.clone(),
        description: step.description.clone(),
        affected_paths: vec![format!("registry:{key}")],
    })
}

fn base64_decode(value: &str) -> Option<Vec<u8>> {
    // Minimal RFC 4648 base64 decoder so we don't pull a new dep just
    // for the write-binary-file step. Returns None on any malformed
    // character so the step fails loudly. Accepts both padded and
    // unpadded inputs.
    const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut cleaned: Vec<u8> = value
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect();
    if cleaned.is_empty() {
        return Some(Vec::new());
    }
    match cleaned.len() % 4 {
        0 => {}
        2 => cleaned.extend_from_slice(b"=="),
        3 => cleaned.push(b'='),
        _ => return None,
    }
    let mut output = Vec::with_capacity(cleaned.len() / 4 * 3);
    let mut buffer: u32 = 0;
    let mut bits: u32 = 0;
    for byte in &cleaned {
        let sextet = match ALPHABET.iter().position(|candidate| *candidate == *byte) {
            Some(index) => index as u32,
            None if *byte == b'=' => continue,
            None => return None,
        };
        buffer = (buffer << 6) | sextet;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push(((buffer >> bits) & 0xFF) as u8);
        }
    }
    Some(output)
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
        assert!(kinds.contains(&"write-binary-file"));
        assert!(kinds.contains(&"move-file"));
        assert!(kinds.contains(&"kill-process"));
        assert!(kinds.contains(&"registry-write"));
        assert!(kinds.contains(&"registry-delete"));
    }

    #[test]
    fn base64_decode_handles_padded_and_unpadded_inputs() {
        let decoded = base64_decode("SGVsbG8=").expect("valid");
        assert_eq!(decoded, b"Hello");
        let decoded = base64_decode("SGVsbG8").expect("unpadded valid");
        assert_eq!(decoded, b"Hello");
        assert!(base64_decode("@@@").is_none());
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