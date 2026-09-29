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
        "download-file",
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
        "download-file" => run_download_file(step, context),
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

fn run_download_file(
    step: &StepSpec,
    context: &StepContext<'_>,
) -> Result<StepResult, String> {
    // The download half of a "fetch a release, then extract it" recipe.
    // Until this kind existed, `extract-zip` could only read a path from
    // config, so every recipe that pointed that field at a URL failed
    // with a file-not-found at install time.
    let url = param_string(step, "urlField")
        .and_then(|field| context.config.get_string(field))
        .or_else(|| param_string(step, "url").map(str::to_owned))
        .ok_or_else(|| "download-file: urlField or url is required.".to_owned())?;
    let target = param_string(step, "targetField")
        .and_then(|field| context.config.get_string(field))
        .or_else(|| param_string(step, "target").map(str::to_owned))
        .ok_or_else(|| "download-file: targetField or target is required.".to_owned())?;

    let parsed = reqwest::Url::parse(&url)
        .map_err(|error| format!("download-file: invalid URL '{url}': {error}"))?;
    if parsed.scheme() != "https" {
        return Err("download-file: only HTTPS is allowed.".to_owned());
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("download-file: credentials in the URL are not allowed.".to_owned());
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| "download-file: URL has no host.".to_owned())?
        .to_ascii_lowercase();
    // Same default allow-list the archive checks use; a recipe that
    // needs another host has to say so explicitly.
    let allowed: Vec<String> = match step.params.get("hostAllowlist") {
        Some(JsonValue::Array(values)) => values
            .iter()
            .filter_map(|value| value.as_str())
            .map(|value| value.to_ascii_lowercase())
            .collect(),
        _ => vec![
            "github.com".to_owned(),
            "objects.githubusercontent.com".to_owned(),
        ],
    };
    if !allowed.iter().any(|candidate| host == *candidate) {
        return Err(format!(
            "download-file: host '{host}' is not in the allow-list ({}) .",
            allowed.join(", ")
        ));
    }

    let client = reqwest::blocking::Client::builder()
        .user_agent("Moddin-Desktop/0.1 (+https://github.com/petonexus/moddin-desktop)")
        .build()
        .map_err(|error| format!("download-file: could not build client: {error}"))?;
    let response = client
        .get(parsed)
        .send()
        .map_err(|error| format!("download-file: request failed: {error}"))?
        .error_for_status()
        .map_err(|error| format!("download-file: server returned an error: {error}"))?;

    let bytes = response
        .bytes()
        .map_err(|error| format!("download-file: could not read response: {error}"))?;
    if bytes.len() as u64 > crate::archive::MAX_ARCHIVE_BYTES {
        return Err(format!(
            "download-file: payload exceeds the {}-byte safety limit.",
            crate::archive::MAX_ARCHIVE_BYTES
        ));
    }

    // A partial or tampered payload must not survive to be extracted.
    // Delete on mismatch so a retry starts clean instead of reusing a
    // file the recipe believes is verified.
    if let Some(expected) = param_string(step, "expectedField")
        .and_then(|field| context.config.get_string(field))
        .or_else(|| param_string(step, "expected").map(str::to_owned))
    {
        let computed = format!("{:x}", Sha256::digest(&bytes));
        if !computed.eq_ignore_ascii_case(&expected) {
            return Err(format!(
                "download-file: SHA-256 mismatch for '{url}' (expected {expected}, got {computed})."
            ));
        }
    }

    let destination = resolve_download_path(context.executable_directory, &target);
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("download-file: could not create '{}': {error}", parent.display()))?;
    }
    fs::write(&destination, &bytes)
        .map_err(|error| format!("download-file: could not write '{}': {error}", destination.display()))?;

    // The downloaded file is a build artifact in the Moddin cache, not
    // something the install touched in the game folder, so it is
    // deliberately not part of the rollback set.
    Ok(StepResult {
        kind: step.kind.as_str().to_owned(),
        description: step.description.clone(),
        affected_paths: Vec::new(),
    })
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
             use archivePath/archivePathField instead."
        ));
    } else if let Some(path) = param_string(step, "archivePathField")
        .and_then(|field| context.config.get_string(field))
        .or_else(|| param_string(step, "archivePath").map(str::to_owned))
    {
        fs::read(resolve_download_path(context.executable_directory, &path)).map_err(|error| {
            format!("extract-zip: could not read archive '{path}': {error}")
        })?
    } else {
        return Err(
            "extract-zip: step needs archiveBytesField, archivePathField or archivePath.".to_owned(),
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

/// Where `download-file` puts payloads and where `extract-zip` looks
/// for them when a recipe names a bare filename.
///
/// Recipes are portable YAML, so they cannot hard-code a user profile.
/// Naming just `bepinex.zip` keeps the archive in Moddin's own cache —
/// never inside the game folder, where it would be picked up by the
/// uninstall/rollback bookkeeping and by mod scanners. A candidate with
/// a directory component keeps the ordinary relative-to-executable-dir
/// meaning.
pub fn download_cache_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .map(|dir| dir.join("Moddin").join("downloads"))
        .unwrap_or_else(|| std::env::temp_dir().join("moddin-downloads"))
}

fn resolve_download_path(base: &Path, candidate: &str) -> PathBuf {
    let path = Path::new(candidate);
    let is_bare_filename = path.components().count() == 1 && path.file_name().is_some();
    if is_bare_filename {
        download_cache_dir().join(path)
    } else {
        resolve_path(base, candidate)
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

    /// Build a `download-file` step around literal params so each guard
    /// can be exercised without touching the network. Every case below is
    /// rejected before the client is ever built.
    fn download_step(params: &[(&str, JsonValue)]) -> StepSpec {
        StepSpec {
            kind: "download-file".to_owned(),
            params: params
                .iter()
                .map(|(key, value)| ((*key).to_owned(), value.clone()))
                .collect(),
            description: None,
        }
    }

    /// A `StepContext` whose only meaningful field is the config. Steps
    /// under test never read the spec.
    fn download_context<'a>(config: &'a ResolvedConfig, root: &'a Path) -> StepContext<'a> {
        let spec: &'a CapabilitySpec = Box::leak(Box::new(CapabilitySpec {
            id: "probe".to_owned(),
            display_name: "Probe".to_owned(),
            category: "qol".to_owned(),
            status: "available".to_owned(),
            supported_engines: Vec::new(),
            dependencies: Vec::new(),
            compatibility: None,
            checks: Vec::new(),
            install: Vec::new(),
            uninstall: Vec::new(),
            verify: Vec::new(),
            safety_notes: Vec::new(),
            config_schema: Vec::new(),
            origin: crate::capability::SpecOrigin::BuiltIn,
        }));
        StepContext {
            spec,
            config,
            install_directory: root,
            executable_directory: root,
        }
    }

    #[test]
    fn download_requires_a_url_and_a_target() {
        let root = std::env::temp_dir();
        let config = ResolvedConfig::default();

        let missing_url = execute_step(
            &download_step(&[("target", json!("out.zip"))]),
            &download_context(&config, &root),
        )
        .expect_err("no url is rejected");
        assert!(missing_url.contains("urlField or url is required"), "{missing_url}");

        let missing_target = execute_step(
            &download_step(&[("url", json!("https://github.com/a/b.zip"))]),
            &download_context(&config, &root),
        )
        .expect_err("no target is rejected");
        assert!(
            missing_target.contains("targetField or target is required"),
            "{missing_target}"
        );
    }

    #[test]
    fn download_refuses_plain_http() {
        let root = std::env::temp_dir();
        let config = ResolvedConfig::default();
        let error = execute_step(
            &download_step(&[
                ("url", json!("http://github.com/a/b.zip")),
                ("target", json!("out.zip")),
            ]),
            &download_context(&config, &root),
        )
        .expect_err("plain HTTP is refused");
        assert!(error.contains("only HTTPS"), "{error}");
    }

    #[test]
    fn download_refuses_embedded_credentials() {
        let root = std::env::temp_dir();
        let config = ResolvedConfig::default();
        let error = execute_step(
            &download_step(&[
                ("url", json!("https://user:secret@github.com/a/b.zip")),
                ("target", json!("out.zip")),
            ]),
            &download_context(&config, &root),
        )
        .expect_err("credentials are refused");
        assert!(error.contains("credentials"), "{error}");
    }

    #[test]
    fn download_refuses_hosts_outside_the_allowlist() {
        let root = std::env::temp_dir();
        let config = ResolvedConfig::default();
        let error = execute_step(
            &download_step(&[
                ("url", json!("https://evil.example.com/a/b.zip")),
                ("target", json!("out.zip")),
            ]),
            &download_context(&config, &root),
        )
        .expect_err("an unlisted host is refused");
        assert!(error.contains("not in the allow-list"), "{error}");
        assert!(
            error.contains("github.com"),
            "the error lists what is allowed: {error}"
        );
    }

    #[test]
    fn download_reads_its_url_and_target_from_config_fields() {
        let root = std::env::temp_dir();
        let mut config = ResolvedConfig::default();
        config.values.insert("downloadUrl".to_owned(), json!("http://example.com/x"));
        config.values.insert("sha256".to_owned(), json!("deadbeef"));

        // The URL comes from config, so the scheme check must still run:
        // a recipe cannot smuggle plain HTTP in through a field.
        let error = execute_step(
            &download_step(&[("urlField", json!("downloadUrl")), ("target", json!("out.zip"))]),
            &download_context(&config, &root),
        )
        .expect_err("config-sourced URLs are validated too");
        assert!(error.contains("only HTTPS"), "{error}");

        // A missing field is a clear error, not a silent default.
        let error = execute_step(
            &download_step(&[("urlField", json!("absentField")), ("target", json!("out.zip"))]),
            &download_context(&config, &root),
        )
        .expect_err("a missing config field is an error");
        assert!(error.contains("urlField or url is required"), "{error}");
    }

    #[test]
    fn bare_archive_names_resolve_into_the_download_cache() {
        let base = Path::new("C:\\games\\Some Game");
        // A recipe is portable YAML, so it names the archive without a
        // profile path. It must not land in the game folder.
        let cached = resolve_download_path(base, "bepinex.zip");
        assert_eq!(cached.parent(), Some(download_cache_dir().as_path()));
        assert!(cached.ends_with("bepinex.zip"));

        // Anything with a directory component keeps the ordinary
        // relative-to-the-game meaning.
        let nested = resolve_download_path(base, "archives/bepinex.zip");
        assert_eq!(nested, base.join("archives").join("bepinex.zip"));

        let absolute = resolve_download_path(base, "D:\\cache\\x.zip");
        assert_eq!(absolute, PathBuf::from("D:\\cache\\x.zip"));
    }

    #[test]
    fn extract_zip_accepts_a_literal_archive_path() {
        let root = std::env::temp_dir();
        let config = ResolvedConfig::default();
        let archive = root.join("moddin-extract-literal-test.zip");
        fs::write(&archive, b"not a zip").expect("write placeholder archive");

        let spec = StepSpec {
            kind: "extract-zip".to_owned(),
            params: BTreeMap::from([("archivePath".to_owned(), json!(archive.to_string_lossy()))]),
            description: None,
        };
        // The literal is found and read; it only fails later, at the zip
        // parser, which is what proves the path was resolved at all.
        let error = execute_step(&spec, &download_context(&config, &root))
            .expect_err("placeholder bytes are not a zip");
        assert!(
            error.contains("could not open zip"),
            "the literal path was used: {error}"
        );

        let _ = fs::remove_file(&archive);
    }
}