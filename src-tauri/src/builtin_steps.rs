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
    capability::{ResolvedConfig, StepSpec},
    path_guard::sanitize_archive_member,
};
use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, VecDeque},
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
        "git-checkout",
        "build-project",
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
    /// Full commit SHA a `git-checkout` actually landed on. Recorded so
    /// the install record names the exact tree that was built rather
    /// than the tag it was asked for — a tag can be moved upstream, a
    /// commit cannot.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_commit: Option<String>,
}

/// Context the runner passes to each step.
///
/// Deliberately does *not* carry the owning `CapabilitySpec`: no step kind
/// reads it, and threading it in only invited steps to reach around the
/// resolved `config` and `install_directory` they are given.
pub struct StepContext<'a> {
    pub config: &'a ResolvedConfig,
    pub install_directory: &'a Path,
    pub executable_directory: &'a Path,
}

/// Execute a single step. Returns the structured result or an error
/// string the UI surfaces verbatim.
pub fn execute_step(step: &StepSpec, context: &StepContext<'_>) -> Result<StepResult, String> {
    match step.kind.as_str() {
        "download-file" => run_download_file(step, context),
        "extract-zip" => run_extract_zip(step, context),
        "git-checkout" => run_git_checkout(step, context),
        "build-project" => run_build_project(step, context),
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

/// Reject a URL whose host the recipe did not name.
///
/// The default allow-list is the same one the archive checks use, so a
/// recipe that needs another host has to say so explicitly. Without it
/// a wrong or tampered recipe can point an install — a download or a
/// clone — at a server nobody reviewed.
fn check_host_allowlist(
    label: &str,
    original: &str,
    parsed: &reqwest::Url,
    step: &StepSpec,
) -> Result<(), String> {
    let host = parsed
        .host_str()
        .ok_or_else(|| format!("{label}: URL has no host."))?
        .to_ascii_lowercase();
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
    if !allowed.contains(&host) {
        return Err(format!(
            "{label}: host '{host}' of '{original}' is not in the allow-list ({}).",
            allowed.join(", ")
        ));
    }
    Ok(())
}

fn run_download_file(step: &StepSpec, context: &StepContext<'_>) -> Result<StepResult, String> {
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
    check_host_allowlist("download-file", &url, &parsed, step)?;

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
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "download-file: could not create '{}': {error}",
                parent.display()
            )
        })?;
    }
    fs::write(&destination, &bytes).map_err(|error| {
        format!(
            "download-file: could not write '{}': {error}",
            destination.display()
        )
    })?;

    // The downloaded file is a build artifact in the Moddin cache, not
    // something the install touched in the game folder, so it is
    // deliberately not part of the rollback set.
    Ok(StepResult {
        kind: step.kind.as_str().to_owned(),
        description: step.description.clone(),
        affected_paths: Vec::new(),
        resolved_commit: None,
    })
}

fn run_extract_zip(step: &StepSpec, context: &StepContext<'_>) -> Result<StepResult, String> {
    // Two layouts supported:
    //   1. archiveBytesField — bytes come from a previously downloaded
    //      buffer held on the spec (not implemented; see below).
    //   2. archivePathField / archivePath — bytes come from a path the
    //      runner resolves via config (local archive).
    //
    // Either way the container is decided by the bytes, never by the
    // name: a recipe is free to stage a `.7z` as `something.zip`, and
    // the OptiScaler release it comes from ships no zip at all.
    //
    // Both formats extract every member of the archive into the target
    // root after sanitising the member path. Members that match the
    // recipe's declared payload DLL take the recipe's chosen proxy name
    // (only when `proxyField` resolves to a non-empty string); a recipe
    // that names no payload falls back to the ReShade-era defaults.
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
        fs::read(resolve_download_path(context.executable_directory, &path))
            .map_err(|error| format!("extract-zip: could not read archive '{path}': {error}"))?
    } else {
        return Err(
            "extract-zip: step needs archiveBytesField, archivePathField or archivePath."
                .to_owned(),
        );
    };

    let format = detect_archive_format(&bytes).ok_or_else(|| {
        format!(
            "extract-zip: could not open zip: '{}' is not a zip or a 7z archive \
             (it looks like {}). Only zip and 7z payloads can be extracted.",
            step_archive_label(step, context),
            describe_archive_format(&bytes)
        )
    })?;

    let root = extract_target_root(step, context)?;
    let proxy = param_string(step, "proxyField").and_then(|field| context.config.get_string(field));
    // Every member, named once, so the executor and the transaction
    // planner resolve the same targets for both formats.
    let names = archive_member_names(&bytes, format)?;
    let payload = declared_payload_member(step, context, &names)?;
    let members = archive_member_targets(&names, proxy.as_deref(), payload.as_deref())?;

    let mut affected = Vec::new();
    for member in &members {
        affected.push(match staging_prefix(step, context.config) {
            Some(prefix) => format!("{prefix}/{}", member.target),
            None => member.target.clone(),
        });
    }

    match format {
        ArchiveFormat::Zip => {
            let mut archive = ZipArchive::new(Cursor::new(bytes.as_slice()))
                .map_err(|error| format!("extract-zip: could not open zip: {error}"))?;
            // `members` was built from the same walk — every non-directory
            // entry, in order — so one cursor keeps the two aligned.
            let mut cursor = 0usize;
            for index in 0..archive.len() {
                let mut entry = archive
                    .by_index(index)
                    .map_err(|error| format!("extract-zip: entry {index} unreadable: {error}"))?;
                if entry.is_dir() {
                    continue;
                }
                let name = entry.name().to_owned();
                // `members` came from the same walk, so this cannot run
                // out — and if it somehow did, writing less than the
                // transaction was promised is not a success.
                let Some(member) = members.get(cursor) else {
                    return Err(format!(
                        "extract-zip: entry {index} ('{name}') has no planned target."
                    ));
                };
                cursor += 1;
                write_extracted_member(&root.join(&member.target), &mut entry, &name)?;
            }
        }
        ArchiveFormat::SevenZ => {
            // The 7z reader hands its entries over in block order, which
            // is not always the order the header lists them, so targets
            // are looked up by the member's own name.
            let mut pending: HashMap<String, VecDeque<String>> = HashMap::new();
            for member in &members {
                pending
                    .entry(member.safe_name.clone())
                    .or_default()
                    .push_back(member.target.clone());
            }
            let mut stopped: Option<String> = None;
            let outcome = sevenz_rust::SevenZReader::new(
                Cursor::new(bytes.as_slice()),
                bytes.len() as u64,
                sevenz_rust::Password::empty(),
            )
            .map_err(|error| format!("extract-zip: could not open 7z: {error}"))?
            .for_each_entries(|entry, reader| {
                if stopped.is_some() {
                    return Ok(false);
                }
                if entry.is_directory() {
                    return Ok(true);
                }
                let name = entry.name().to_owned();
                let Some(safe_name) = sanitize_archive_member(&name) else {
                    stopped = Some(format!("extract-zip: unsafe archive member '{name}'."));
                    return Ok(false);
                };
                let Some(relative) = pending.get_mut(&safe_name).and_then(VecDeque::pop_front)
                else {
                    // Not a member this step planned; the transaction was
                    // handed a path that will not exist.
                    stopped = Some(format!(
                        "extract-zip: the archive yielded a member '{name}' that was not planned."
                    ));
                    return Ok(false);
                };
                let target = root.join(&relative);
                match write_extracted_member(&target, reader, &name) {
                    Ok(()) => Ok(true),
                    Err(error) => {
                        stopped = Some(error);
                        Ok(false)
                    }
                }
            });
            if let Some(error) = stopped {
                return Err(error);
            }
            outcome.map_err(|error| format!("extract-zip: could not read 7z: {error}"))?;
        }
    }

    Ok(StepResult {
        kind: step.kind.clone(),
        description: step.description.clone(),
        affected_paths: affected,
        resolved_commit: None,
    })
}

/// A member that has been read out of an archive, written to its final
/// path. Shared by both containers so a zip member and a 7z member get
/// the same parent-directory and error treatment.
fn write_extracted_member(target: &Path, reader: &mut dyn Read, name: &str) -> Result<(), String> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!("extract-zip: could not create parent dir for '{name}': {error}")
        })?;
    }
    let mut buffer = Vec::new();
    reader
        .read_to_end(&mut buffer)
        .map_err(|error| format!("extract-zip: could not read '{name}': {error}"))?;
    let mut file = File::create(target)
        .map_err(|error| format!("extract-zip: could not create target: {error}"))?;
    file.write_all(&buffer)
        .map_err(|error| format!("extract-zip: could not write target: {error}"))
}

/// The archive's own location, for an error message. Recipe authors read
/// the path they wrote, not the resolved cache path the runner used.
fn step_archive_label(step: &StepSpec, context: &StepContext<'_>) -> String {
    param_string(step, "archivePathField")
        .and_then(|field| context.config.get_string(field))
        .or_else(|| param_string(step, "archivePath").map(str::to_owned))
        .unwrap_or_else(|| "<downloaded buffer>".to_owned())
}

/// Container formats the `extract-zip` step can open. Nothing else is in
/// scope: `zip` and `sevenz-rust` are the only decoders the crate
/// depends on, and adding a third is a dependency decision, not a
/// detail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArchiveFormat {
    Zip,
    SevenZ,
}

/// Decide the container from the leading bytes.
///
/// Magic bytes, not the file name. A recipe's `target` /
/// `archivePath` is the author's choice, and in these recipes it is
/// demonstrably wrong: the OptiScaler release publishes one `.7z` asset
/// that the recipe stages as `optiscaler.zip`. Both decoders are handed
/// a byte slice and open what they recognise, so sniffing first is what
/// lets the step hand the payload to the reader that can read it.
fn detect_archive_format(bytes: &[u8]) -> Option<ArchiveFormat> {
    // Local file header, or the end-of-central-directory record an
    // archive with no members starts with.
    const ZIP: [&[u8]; 2] = [b"PK\x03\x04", b"PK\x05\x06"];
    // `7z` + version 0.2 + start header CRC.
    const SEVEN_Z: &[u8] = b"7z\xBC\xAF\x27\x1C";

    if bytes.starts_with(SEVEN_Z) {
        Some(ArchiveFormat::SevenZ)
    } else if ZIP.iter().any(|magic| bytes.starts_with(magic)) {
        Some(ArchiveFormat::Zip)
    } else {
        None
    }
}

/// Name a payload this step cannot open, for the refusal message.
///
/// A step that installs nothing and reports success is the worst
/// outcome available, so an unrecognised container has to say what it
/// actually is rather than fail with a decoder's internal error.
fn describe_archive_format(bytes: &[u8]) -> String {
    const KNOWN: [(&[u8], &str); 7] = [
        (b"\x1f\x8b", "gzip"),
        (b"Rar!\x1a\x07\x00", "rar"),
        (b"Rar!\x1a\x07\x01\x00", "rar5"),
        (b"\xFD7zXZ\x00", "xz"),
        (b"BZh", "bzip2"),
        (b"\x28\xB5\x2F\xFD", "zstd"),
        (b"\x04\x22\x4D\x18", "lz4"),
    ];
    if let Some((_, name)) = KNOWN.iter().find(|(magic, _)| bytes.starts_with(magic)) {
        return (*name).to_owned();
    }
    if bytes.get(257..262) == Some(b"ustar") {
        return "tar".to_owned();
    }
    let head: Vec<String> = bytes
        .iter()
        .take(6)
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if head.is_empty() {
        "an empty file".to_owned()
    } else {
        format!("an unrecognised container starting with {}", head.join(" "))
    }
}

/// Every non-directory member of an archive, in archive order.
fn archive_member_names(bytes: &[u8], format: ArchiveFormat) -> Result<Vec<String>, String> {
    match format {
        ArchiveFormat::Zip => {
            let mut archive = ZipArchive::new(Cursor::new(bytes))
                .map_err(|error| format!("extract-zip: could not open zip: {error}"))?;
            let mut names = Vec::with_capacity(archive.len());
            for index in 0..archive.len() {
                let entry = archive
                    .by_index(index)
                    .map_err(|error| format!("extract-zip: entry {index} unreadable: {error}"))?;
                if !entry.is_dir() {
                    names.push(entry.name().to_owned());
                }
            }
            Ok(names)
        }
        ArchiveFormat::SevenZ => {
            let reader = sevenz_rust::SevenZReader::new(
                Cursor::new(bytes),
                bytes.len() as u64,
                sevenz_rust::Password::empty(),
            )
            .map_err(|error| format!("extract-zip: could not open 7z: {error}"))?;
            Ok(reader
                .archive()
                .files
                .iter()
                .filter(|entry| !entry.is_directory())
                .map(|entry| entry.name().to_owned())
                .collect())
        }
    }
}

/// The member that takes the recipe's proxy name, if the recipe named
/// one with `payload` / `payloadField`.
///
/// The recipe names the main payload DLL — the single file the archive
/// exists to install, the one that has to land next to the executable
/// under a name the game will load. When the archive carries more than
/// one member with that name, the shallowest one is the payload, which
/// is the rule the dedicated OptiScaler module already applies in
/// `find_payload_root`: the copy at the archive root is the payload and
/// anything deeper is a bundled sample.
///
/// A declared payload the archive does not ship is an error. Extracting
/// the rest and reporting success would install a mod that loads
/// nothing.
fn declared_payload_member(
    step: &StepSpec,
    context: &StepContext<'_>,
    safe_names: &[String],
) -> Result<Option<String>, String> {
    let Some(wanted) = param_string(step, "payloadField")
        .and_then(|field| context.config.get_string(field))
        .or_else(|| param_string(step, "payload").map(str::to_owned))
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty())
    else {
        return Ok(None);
    };

    let mut candidates = safe_names
        .iter()
        .enumerate()
        .filter(|(_, safe_name)| {
            Path::new(safe_name.as_str())
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.eq_ignore_ascii_case(&wanted))
        })
        .collect::<Vec<_>>();
    // Shallowest wins; archive order breaks a tie, so the choice does
    // not depend on how a reader happened to enumerate the members.
    candidates.sort_by_key(|(index, safe_name)| {
        (Path::new(safe_name.as_str()).components().count(), *index)
    });
    match candidates.into_iter().next() {
        Some((_, safe_name)) => Ok(Some(safe_name.clone())),
        None => Err(format!(
            "extract-zip: the recipe names '{wanted}' as the archive's payload DLL, but no \
             member of the archive has that name."
        )),
    }
}

/// One archive member, paired with the path it will be written to.
struct ArchiveMember {
    /// The member's own name, after `sanitize_archive_member`.
    safe_name: String,
    /// Where it lands, with the proxy rename applied.
    target: String,
}

/// Resolve every member of an archive to the path it is written to, in
/// the same order as `names`, after sanitising and the optional proxy
/// rename.
fn archive_member_targets(
    names: &[String],
    proxy: Option<&str>,
    payload: Option<&str>,
) -> Result<Vec<ArchiveMember>, String> {
    let mut members = Vec::with_capacity(names.len());
    for name in names {
        let Some(safe_name) = sanitize_archive_member(name) else {
            return Err(format!("extract-zip: unsafe archive member '{name}'."));
        };
        let target = archive_member_target(&safe_name, proxy, payload);
        members.push(ArchiveMember { safe_name, target });
    }
    Ok(members)
}

/// The `targetSubdirField` / `targetSubdir` value an `extract-zip` step
/// declared, rendered from config, or `None` when it declared neither.
///
/// A recipe that wants its payload staged somewhere other than the game
/// folder — Moddin's own tools directory, a temp dir — says so here.
/// Split out so the executor and [`plan_step_targets`] agree on where a
/// staged archive lands: the transaction has to back up the same files,
/// at the same paths, the step is about to write.
fn staging_prefix(step: &StepSpec, config: &ResolvedConfig) -> Option<String> {
    param_string(step, "targetSubdirField")
        .and_then(|field| config.get_string(field))
        .or_else(|| param_string(step, "targetSubdir").map(str::to_owned))
        .map(|value| render_template(&value, config))
}

/// Directory an `extract-zip` step writes into. No side effects, so
/// [`plan_step_targets`] can predict the same paths before the step runs.
fn resolve_staging_root(step: &StepSpec, context: &StepContext<'_>) -> Result<PathBuf, String> {
    let Some(subdir) = staging_prefix(step, context.config) else {
        // No subdirectory declared: the pre-existing behaviour, straight
        // into the executable directory.
        return Ok(context.executable_directory.to_path_buf());
    };
    if subdir.trim().is_empty() {
        return Err("extract-zip: targetSubdir is empty.".to_owned());
    }
    // The same containment rule every other write target obeys: an
    // absolute path is honoured (a `path`-typed config field exists
    // precisely to name a staging location outside the game folder), a
    // `..` that walks out of the executable directory is refused.
    resolve_write_target(step, context.executable_directory, &subdir)
}

fn extract_target_root(step: &StepSpec, context: &StepContext<'_>) -> Result<PathBuf, String> {
    let root = resolve_staging_root(step, context)?;
    if staging_prefix(step, context.config).is_none() {
        return fs::create_dir_all(&root)
            .map(|()| root)
            .map_err(|error| format!("extract-zip: could not create target directory: {error}"));
    }
    fs::create_dir_all(&root).map_err(|error| {
        format!(
            "extract-zip: could not create staging directory '{}': {error}",
            root.display()
        )
    })?;
    Ok(root)
}

/// What a `git-checkout` step is allowed to pin a repository to.
///
/// Both variants name exactly one immutable object. A branch, `HEAD` or
/// any other floating name is refused: the same recipe run twice has to
/// produce the same tree, and a branch can move between the two runs.
#[derive(Debug, Clone, PartialEq)]
enum PinnedRef {
    /// A full 40-character commit SHA, lowercased.
    Commit(String),
    /// A tag name, with any `refs/tags/` prefix removed.
    Tag(String),
}

/// Refuse anything that is not an exact tag or a full commit SHA.
///
/// The rules are `git check-ref-format` reduced to the ways a recipe
/// author can actually break a checkout, plus the one rule that matters
/// most here: a short SHA is refused rather than expanded, because two
/// different commits can share a seven-character prefix and the install
/// would stop being reproducible.
fn classify_pinned_ref(reference: &str) -> Result<PinnedRef, String> {
    let trimmed = reference.trim();
    if trimmed.is_empty() {
        return Err("git-checkout: ref is empty.".to_owned());
    }

    let looks_hexadecimal = trimmed.len() >= 4
        && trimmed
            .chars()
            .all(|character| character.is_ascii_hexdigit());
    if trimmed.len() == 40 && looks_hexadecimal {
        return Ok(PinnedRef::Commit(trimmed.to_ascii_lowercase()));
    }
    if looks_hexadecimal {
        return Err(format!(
            "git-checkout: ref '{trimmed}' is an abbreviated commit. Pin the full \
             40-character SHA so the install reproduces."
        ));
    }
    if trimmed.starts_with("refs/heads/") || trimmed.starts_with("heads/") {
        return Err(format!(
            "git-checkout: ref '{trimmed}' is a branch. Pin a tag (refs/tags/<tag>) or the \
             full 40-character commit SHA — a branch can move after the install."
        ));
    }
    if trimmed.eq_ignore_ascii_case("head") {
        return Err(format!(
            "git-checkout: ref '{trimmed}' is floating. Pin a tag or a full commit SHA."
        ));
    }
    if trimmed.contains('/') && !trimmed.starts_with("refs/tags/") {
        return Err(format!(
            "git-checkout: ref '{trimmed}' is ambiguous. Write the tag in full \
             (refs/tags/<tag>) or pin the 40-character commit SHA."
        ));
    }

    let name = trimmed.strip_prefix("refs/tags/").unwrap_or(trimmed);
    if name.is_empty() || name.starts_with('-') || name.ends_with('/') || name.ends_with(".lock") {
        return Err(format!(
            "git-checkout: ref '{trimmed}' is not a valid tag name."
        ));
    }
    let forbidden = name.contains("..")
        || name.contains("@{")
        || name.chars().any(|character| {
            matches!(
                character,
                '~' | '^' | ':' | '?' | '*' | '[' | '\\' | ' ' | '\t'
            ) || character.is_control()
        });
    if forbidden {
        return Err(format!(
            "git-checkout: ref '{trimmed}' contains a character git does not allow in a ref \
             name."
        ));
    }
    Ok(PinnedRef::Tag(name.to_owned()))
}

fn run_git_checkout(step: &StepSpec, context: &StepContext<'_>) -> Result<StepResult, String> {
    // The step a recipe reaches for when its payload is a git tag rather
    // than a release archive: there is no zip for `download-file` to
    // fetch and nothing for `extract-zip` to open, so the recipe checks
    // the source out and builds it.
    let repository = param_string(step, "repoField")
        .and_then(|field| context.config.get_string(field))
        .or_else(|| param_string(step, "repo").map(str::to_owned))
        .ok_or("git-checkout: repoField or repo is required.")?;
    let reference = param_string(step, "refField")
        .and_then(|field| context.config.get_string(field))
        .or_else(|| param_string(step, "ref").map(str::to_owned))
        .ok_or("git-checkout: refField or ref is required.")?;
    let target = param_string(step, "targetField")
        .and_then(|field| context.config.get_string(field))
        .or_else(|| param_string(step, "target").map(str::to_owned))
        .ok_or("git-checkout: targetField or target is required.")?;

    // The same three guards `download-file` applies to a release URL,
    // for the same reasons: a recipe must not be able to turn an
    // install into a plaintext download, a credential prompt, or a
    // request to a server the recipe never named.
    let parsed = reqwest::Url::parse(&repository)
        .map_err(|error| format!("git-checkout: invalid repository URL '{repository}': {error}"))?;
    if parsed.scheme() != "https" {
        return Err("git-checkout: only HTTPS is allowed.".to_owned());
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("git-checkout: credentials in the repository URL are not allowed.".to_owned());
    }
    check_host_allowlist("git-checkout", &repository, &parsed, step)?;

    let fetch_ref = match classify_pinned_ref(&reference)? {
        PinnedRef::Commit(sha) => sha,
        // Namespaced on the remote so a branch of the same name can
        // never satisfy the fetch.
        PinnedRef::Tag(name) => format!("refs/tags/{name}"),
    };

    let directory = resolve_write_target(
        step,
        context.install_directory,
        &render_template(&target, context.config),
    )?;
    prepare_checkout_root(&directory, &repository)?;

    // Shallow on purpose: a mod install does not need the project's
    // history, and a full clone of UEVR is gigabytes.
    let root = context.install_directory.to_path_buf();
    let staged = directory.to_string_lossy().into_owned();
    run_git(&["init", "--quiet", &staged], &root)?;
    run_git(
        &[
            "-C",
            &staged,
            "remote",
            "add",
            "origin",
            repository.as_str(),
        ],
        &root,
    )?;
    run_git(
        &[
            "-C", &staged, "fetch", "--quiet", "--depth", "1", "origin", &fetch_ref,
        ],
        &root,
    )?;
    run_git(
        &[
            "-C",
            &staged,
            "checkout",
            "--quiet",
            "--detach",
            "FETCH_HEAD",
        ],
        &root,
    )?;

    // The tag is what the recipe asked for; the commit is what it got.
    // Recording the commit is the only way a later reader can tell
    // which tree was built, because a tag can be moved upstream.
    let commit = run_git(&["-C", &staged, "rev-parse", "HEAD"], &root)?;
    if commit.len() != 40 || !commit.chars().all(|value| value.is_ascii_hexdigit()) {
        return Err(format!(
            "git-checkout: git reported '{commit}' rather than a commit SHA."
        ));
    }

    Ok(StepResult {
        kind: step.kind.clone(),
        description: step.description.clone(),
        // A source tree is not game content: it is deliberately left out
        // of the rollback set, which is what keeps a repository-sized
        // checkout out of the transaction backups.
        affected_paths: Vec::new(),
        resolved_commit: Some(commit),
    })
}

fn run_build_project(step: &StepSpec, context: &StepContext<'_>) -> Result<StepResult, String> {
    // The per-backend half of a `git-checkout`: a project that publishes
    // a source tag and no prebuilt archive has to be compiled here, and
    // the recipe says which variant it wants and what that variant
    // produces. It reuses the process runner `spawn-process` uses rather
    // than a second one.
    let directory = build_working_directory(step, context)?;
    let command = path_param(step, context.config, "commandField", "command")?;
    let program = resolve_build_program(step, &directory, &command)?;
    let outputs = declared_outputs(step, context.config)?;

    let arguments: Vec<String> = param(step, "args")
        .and_then(|value| value.as_array())
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str())
                .map(|value| render_template(value, context.config))
                .collect()
        })
        .unwrap_or_default();
    let argument_refs: Vec<&str> = arguments.iter().map(String::as_str).collect();
    run_command("build-project", &program, &argument_refs, &directory, &[])?;

    // A build that exits 0 but writes nothing the recipe declared has
    // not succeeded, and saying so here is the difference between a
    // clear failure and a mod that silently does nothing.
    let mut affected = Vec::new();
    for output in &outputs {
        let target = resolve_write_target(step, &directory, output)?;
        if !target.exists() {
            return Err(format!(
                "build-project: '{command}' exited successfully but did not produce '{output}' \
                 in {}.",
                directory.display()
            ));
        }
        affected.push(target.to_string_lossy().into_owned());
    }

    Ok(StepResult {
        kind: step.kind.clone(),
        description: step.description.clone(),
        affected_paths: affected,
        resolved_commit: None,
    })
}

/// Directory a `build-project` step runs in: the tree a previous
/// `git-checkout` step created, named by the recipe rather than guessed.
fn build_working_directory(step: &StepSpec, context: &StepContext<'_>) -> Result<PathBuf, String> {
    let declared = path_param(step, context.config, "directoryField", "directory")?;
    let directory = resolve_write_target(step, context.install_directory, &declared)?;
    if !directory.is_dir() {
        return Err(format!(
            "build-project: '{declared}' is not a directory. A build runs inside the tree a \
             git-checkout step checked out."
        ));
    }
    Ok(directory)
}

/// The program a `build-project` step runs.
///
/// A command with a path component is resolved inside the checkout and
/// must be a real file, the same rule `spawn-process` applies. A bare
/// name is left to the OS to find on `PATH`, exactly as it would be in
/// the user's own shell — a recipe that needs `msbuild` should not have
/// to ship it.
fn resolve_build_program(
    _step: &StepSpec,
    working_directory: &Path,
    command: &str,
) -> Result<PathBuf, String> {
    if command.contains('/') || command.contains('\\') {
        let candidate = resolve_path(working_directory, command);
        if !candidate.is_file() {
            return Err(format!(
                "build-project: '{command}' is not a file in {}.",
                working_directory.display()
            ));
        }
        return Ok(candidate);
    }
    Ok(PathBuf::from(command))
}

/// Files a `build-project` step says it produces, rendered from config.
///
/// Required, not optional: without it the step cannot tell a successful
/// build from a no-op, and the transaction has nothing to record.
fn declared_outputs(step: &StepSpec, config: &ResolvedConfig) -> Result<Vec<String>, String> {
    let required = "build-project: outputs is required — a build has to declare what it \
                    produced so the transaction can track it.";
    let raw = param(step, "outputs")
        .and_then(|value| value.as_array())
        .ok_or_else(|| required.to_owned())?;
    let outputs: Vec<String> = raw
        .iter()
        .filter_map(|value| value.as_str())
        .map(|value| render_template(value, config))
        .filter(|value| !value.trim().is_empty())
        .collect();
    if outputs.is_empty() {
        return Err(required.to_owned());
    }
    Ok(outputs)
}

/// Make the checkout target safe to write into before `git init` does.
///
/// Moddin replaces a directory it created and refuses one it did not —
/// the same rule `ofxr.rs` applies to its own install directory.
/// Ownership is proven by the checkout's own recorded `origin`, not by
/// its name, so a folder that merely happens to be called `uevr-src`
/// is never deleted.
fn prepare_checkout_root(directory: &Path, repository: &str) -> Result<(), String> {
    if !directory.exists() {
        return match directory.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => {
                fs::create_dir_all(parent).map_err(|error| {
                    format!(
                        "git-checkout: could not create '{}': {error}",
                        parent.display()
                    )
                })
            }
            _ => Ok(()),
        };
    }
    if !directory.is_dir() {
        return Err(format!(
            "git-checkout: '{}' already exists and is not a directory.",
            directory.display()
        ));
    }
    if !checkout_origin(directory)
        .map(|origin| origin.trim() == repository)
        .unwrap_or(false)
    {
        return Err(format!(
            "git-checkout: '{}' already exists and was not checked out from this recipe. \
             Delete it or point `target` elsewhere — Moddin will not overwrite a directory \
             it does not own.",
            directory.display()
        ));
    }
    fs::remove_dir_all(directory).map_err(|error| {
        format!(
            "git-checkout: could not replace the previous checkout '{}': {error}",
            directory.display()
        )
    })
}

/// `remote.origin.url` of an existing checkout, or `None` when the
/// directory is not one.
fn checkout_origin(directory: &Path) -> Option<String> {
    let staged = directory.to_string_lossy().into_owned();
    let mut command = std::process::Command::new("git");
    crate::process::HideConsole::hide_console(&mut command);
    let output = command
        .args([
            "-C",
            staged.as_str(),
            "config",
            "--get",
            "remote.origin.url",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// `git` with the two settings that keep a checkout from hanging on a
/// prompt. `GIT_TERMINAL_PROMPT=0` and an empty askpass mean a private
/// key can never open a dialog behind an installer: the command fails,
/// and the failure is reported.
fn run_git(args: &[&str], working_directory: &Path) -> Result<String, String> {
    let output = run_command(
        "git-checkout",
        Path::new("git"),
        args,
        working_directory,
        &[
            ("GIT_TERMINAL_PROMPT", "0"),
            ("GIT_ASKPASS", ""),
            ("SSH_ASKPASS", ""),
        ],
    )?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// Run a program to completion, off-screen, and hand back its output.
///
/// The single process runner for every step that has to *read* a
/// command's result. `spawn-process` goes through [`spawn_detached`]
/// instead because its child has to outlive the install; both hide the
/// console so nothing flashes at the user mid-install.
fn run_command(
    label: &str,
    program: &Path,
    args: &[&str],
    working_directory: &Path,
    env: &[(&str, &str)],
) -> Result<std::process::Output, String> {
    let mut command = std::process::Command::new(program);
    crate::process::HideConsole::hide_console(&mut command);
    command.args(args).current_dir(working_directory);
    for (key, value) in env {
        command.env(key, value);
    }
    let output = command
        .output()
        .map_err(|error| format!("{label}: could not run '{}': {error}", program.display()))?;
    if !output.status.success() {
        return Err(format!(
            "{label}: '{}' exited {}: {}",
            program.display(),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(output)
}

/// Start a program and let it outlive the install.
fn spawn_detached(
    label: &str,
    program: &Path,
    working_directory: &Path,
    env: &[(&str, &str)],
) -> Result<(), String> {
    let mut command = std::process::Command::new(program);
    crate::process::HideConsole::hide_console(&mut command);
    command.current_dir(working_directory);
    for (key, value) in env {
        command.env(key, value);
    }
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("{label}: could not start '{}': {error}", program.display()))
}

fn run_verify_hash(step: &StepSpec, context: &StepContext<'_>) -> Result<StepResult, String> {
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
        resolved_commit: None,
    })
}

/// Read a step's target path.
///
/// The `*Field` form names a config field; the bare form is a literal.
/// Both are accepted so a recipe can point at a constant path without
/// inventing a config field for it — the same rule `download-file` and
/// `verify-hash` already follow, and the one
/// [`plan_step_targets`] mirrors when it predicts the same target.
///
/// Both forms are rendered through [`render_template`] first, the same
/// way `write-text-file` renders its body. A recipe that stages a file
/// and then promotes it needs to say "the file of that name inside the
/// directory I staged into", and neither form can express that without
/// composing `{stagingDir}/{fileName}`. Rendering is a no-op for every
/// path that holds no `{...}`, so no shipped recipe changes meaning — and
/// the community validator already requires a `{name}` in a step param
/// to name a declared config field, so this is the behaviour the author
/// was already promised.
fn path_param(
    step: &StepSpec,
    config: &ResolvedConfig,
    field_key: &str,
    literal_key: &str,
) -> Result<String, String> {
    let raw = if let Some(name) = param_string(step, field_key) {
        config
            .get_string(name)
            .ok_or_else(|| format!("{}: config field '{name}' is missing.", step.kind))?
    } else {
        param_string(step, literal_key)
            .map(str::to_owned)
            .ok_or_else(|| format!("{}: {field_key} or {literal_key} is required.", step.kind))?
    };
    Ok(render_template(&raw, config))
}

fn run_file_delete(step: &StepSpec, context: &StepContext<'_>) -> Result<StepResult, String> {
    let path = path_param(step, context.config, "pathField", "path")?;
    let resolved = resolve_write_target(step, context.executable_directory, &path)?;
    if !resolved.is_file() {
        return Err(format!("file-delete: '{path}' is not a regular file."));
    }
    fs::remove_file(&resolved)
        .map_err(|error| format!("file-delete: could not remove '{path}': {error}"))?;
    Ok(StepResult {
        kind: step.kind.clone(),
        description: step.description.clone(),
        affected_paths: vec![resolved.to_string_lossy().into_owned()],
        resolved_commit: None,
    })
}

fn run_write_text_file(step: &StepSpec, context: &StepContext<'_>) -> Result<StepResult, String> {
    let path = path_param(step, context.config, "pathField", "path")?;
    let resolved = resolve_write_target(step, context.executable_directory, &path)?;
    if let Some(parent) = resolved.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("write-text-file: could not create parent dir: {error}"))?;
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
        resolved_commit: None,
    })
}

fn run_spawn_process(step: &StepSpec, context: &StepContext<'_>) -> Result<StepResult, String> {
    let executable = path_param(step, context.config, "executableField", "executable")?;
    let resolved = resolve_path(context.executable_directory, &executable);
    if !resolved.is_file() {
        return Err(format!(
            "spawn-process: '{executable}' is not a regular file at {}.",
            resolved.display()
        ));
    }
    // The child is deliberately detached: the recipe starts a tray
    // process that must outlive the install. Moddin tracks its liveness
    // through process snapshots, so the PID is not carried onward —
    // putting it in `affected_paths` would make a PID look like a file.
    let working_directory = resolved
        .parent()
        .unwrap_or(context.executable_directory)
        .to_path_buf();
    spawn_detached("spawn-process", &resolved, &working_directory, &[])?;
    Ok(StepResult {
        kind: step.kind.clone(),
        description: step.description.clone(),
        affected_paths: vec![resolved.to_string_lossy().into_owned()],
        resolved_commit: None,
    })
}

fn run_write_binary_file(step: &StepSpec, context: &StepContext<'_>) -> Result<StepResult, String> {
    let path = path_param(step, context.config, "pathField", "path")?;
    let resolved = resolve_write_target(step, context.executable_directory, &path)?;
    if let Some(parent) = resolved.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("write-binary-file: could not create parent dir: {error}"))?;
    }
    let bytes = param_string(step, "base64")
        .and_then(base64_decode)
        .ok_or_else(|| "write-binary-file: base64 param with required hex.".to_owned())?;
    fs::write(&resolved, &bytes)
        .map_err(|error| format!("write-binary-file: could not write '{path}': {error}"))?;
    Ok(StepResult {
        kind: step.kind.clone(),
        description: step.description.clone(),
        affected_paths: vec![resolved.to_string_lossy().into_owned()],
        resolved_commit: None,
    })
}

fn run_move_file(step: &StepSpec, context: &StepContext<'_>) -> Result<StepResult, String> {
    let from_path = path_param(step, context.config, "fromField", "from")?;
    let to_path = path_param(step, context.config, "toField", "to")?;
    let from = resolve_write_target(step, context.executable_directory, &from_path)?;
    let to = resolve_write_target(step, context.executable_directory, &to_path)?;
    if from == to {
        return Err(format!(
            "move-file: source and destination are the same path ('{from_path}')."
        ));
    }
    if from.is_dir() {
        // A directory is not something the transaction store can put
        // back: `snapshot_target` only copies regular files, so a
        // destination that already held a directory would be gone for
        // good and Undo would restore nothing. Refuse instead of
        // replacing, and name the directory the user has to remove.
        if to.exists() {
            return Err(format!(
                "move-file: destination '{to_path}' already exists. A directory move does \
                 not replace it, and Undo cannot restore a directory it never copied — \
                 remove it and try again."
            ));
        }
        // A single rename is atomic on one volume and fails cleanly
        // across two, so the staged tree is never half-copied.
    }
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("move-file: could not create destination parent: {error}"))?;
    }
    fs::rename(&from, &to).map_err(|error| {
        format!("move-file: could not move '{from_path}' -> '{to_path}': {error}")
    })?;
    Ok(StepResult {
        kind: step.kind.clone(),
        description: step.description.clone(),
        affected_paths: vec![
            from.to_string_lossy().into_owned(),
            to.to_string_lossy().into_owned(),
        ],
        resolved_commit: None,
    })
}

fn run_kill_process(step: &StepSpec, _context: &StepContext<'_>) -> Result<StepResult, String> {
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
        return Err(format!("kill-process: taskkill exited {code}: {stderr}"));
    }
    Ok(StepResult {
        kind: step.kind.clone(),
        description: step.description.clone(),
        affected_paths: Vec::new(),
        resolved_commit: None,
    })
}

/// Value types `reg.exe` will not write without a payload.
///
/// `reg add` answers an empty `/d` with "invalid syntax" for all three,
/// and a `REG_DWORD` has no number to store when `/d` is left off
/// entirely, so a recipe that asks for one of these without data is told
/// which param is missing instead of being handed reg.exe's parse error.
const REGISTRY_TYPES_NEEDING_DATA: [&str; 3] = ["REG_DWORD", "REG_MULTI_SZ", "REG_BINARY"];

/// Read a registry step param and render it through [`render_template`].
///
/// A per-game registry key is *always* templated — the per-game name
/// lives in the key, and the key is a string the recipe composes — so
/// `key` and `value` are read the way every other step reads a param and
/// rendered the way `write-text-file` renders its body.
fn registry_param(step: &StepSpec, config: &ResolvedConfig, name: &str) -> Option<String> {
    param_string(step, name).map(|raw| render_template(raw, config))
}

/// The value's payload: a literal `data` or the config field `dataField`
/// names, either way rendered. Absent on purpose — an empty payload is
/// what the OpenXR implicit-layer convention writes, where the value
/// *name* is the manifest path and the data is empty.
///
/// The field is read through [`config_scalar`], so a `number` or `bool`
/// field can carry a `REG_DWORD` payload as well as a string can.
fn registry_data_param(step: &StepSpec, config: &ResolvedConfig) -> Result<Option<String>, String> {
    if let Some(field) = param_string(step, "dataField") {
        let value = config
            .get(field)
            .and_then(config_scalar)
            .ok_or_else(|| format!("{}: config field '{field}' is missing.", step.kind))?;
        return Ok(Some(render_template(&value, config)));
    }
    Ok(registry_param(step, config, "data"))
}

/// Is this key — or, with `value`, this one value under it — in the
/// registry right now?
///
/// `reg query` exits non-zero for a missing key and for a missing value
/// alike, and the message that tells the two apart is localised, so the
/// exit code is the only signal a step can read.
fn registry_target_exists(key: &str, value: Option<&str>) -> bool {
    let mut command = std::process::Command::new("reg.exe");
    crate::process::HideConsole::hide_console(&mut command);
    command.arg("query").arg(key);
    if let Some(name) = value {
        command.arg("/v").arg(name);
    }
    command
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn run_registry_write(step: &StepSpec, context: &StepContext<'_>) -> Result<StepResult, String> {
    let key = registry_param(step, context.config, "key")
        .ok_or_else(|| "registry-write: key is required.".to_owned())?;
    let value = registry_param(step, context.config, "value")
        .ok_or_else(|| "registry-write: value is required.".to_owned())?;
    let data = registry_data_param(step, context.config)?.unwrap_or_default();
    let value_kind = param_string(step, "type").unwrap_or("REG_SZ");
    let force = param(step, "force")
        .and_then(|value| value.as_bool())
        .unwrap_or(true);

    if value_kind != "REG_SZ"
        && value_kind != "REG_EXPAND_SZ"
        && value_kind != "REG_DWORD"
        && value_kind != "REG_BINARY"
        && value_kind != "REG_MULTI_SZ"
    {
        return Err(format!(
            "registry-write: unsupported value type '{value_kind}'."
        ));
    }
    if data.is_empty() && REGISTRY_TYPES_NEEDING_DATA.contains(&value_kind) {
        return Err(format!(
            "registry-write: type {value_kind} has no value to store; set data or dataField."
        ));
    }

    let mut command = std::process::Command::new("reg.exe");
    crate::process::HideConsole::hide_console(&mut command);
    command
        .arg("add")
        .arg(&key)
        .arg("/v")
        .arg(&value)
        .arg("/t")
        .arg(value_kind);
    // `/d` is passed only when the recipe carries a payload. A recipe
    // that declares no data gets the empty-value write it got before
    // this param existed — which is what the OpenXR implicit-layer
    // convention needs, and the only registry write that ever shipped.
    if !data.is_empty() {
        command.arg("/d").arg(data);
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
        resolved_commit: None,
    })
}

fn run_registry_delete(step: &StepSpec, context: &StepContext<'_>) -> Result<StepResult, String> {
    let key = registry_param(step, context.config, "key")
        .ok_or_else(|| "registry-delete: key is required.".to_owned())?;
    let value = registry_param(step, context.config, "value");
    let force = param(step, "force")
        .and_then(|value| value.as_bool())
        .unwrap_or(true);

    let mut command = std::process::Command::new("reg.exe");
    crate::process::HideConsole::hide_console(&mut command);
    command.arg("delete").arg(&key);
    if let Some(name) = &value {
        command.arg("/v").arg(name);
    }
    if force {
        command.arg("/f");
    }
    let output = command
        .output()
        .map_err(|error| format!("registry-delete: could not spawn reg.exe: {error}"))?;
    if !output.status.success() {
        // `reg delete` exits 1 for "there was nothing to delete" and for
        // a real failure alike, and the message that tells them apart is
        // localised. Asking the registry is the locale-independent way to
        // tell them: a target that is verifiably gone has been deleted,
        // whatever reg.exe called it, and an uninstall chain has to stay
        // runnable twice. A target that is still there is a real error.
        if !registry_target_exists(&key, value.as_deref()) {
            return Ok(StepResult {
                kind: step.kind.clone(),
                description: step.description.clone(),
                affected_paths: vec![format!("registry:{key}")],
                resolved_commit: None,
            });
        }
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
        resolved_commit: None,
    })
}

fn base64_decode(value: &str) -> Option<Vec<u8>> {
    // Minimal RFC 4648 base64 decoder so we don't pull a new dep just
    // for the write-binary-file step. Returns None on any malformed
    // character so the step fails loudly. Accepts both padded and
    // unpadded inputs.
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
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

/// The string a config value renders to, for the scalar kinds a recipe
/// can declare in `configSchema`.
///
/// `None` for anything else — an object, an array, a fractional number —
/// so a template leaves its placeholder in place instead of writing half
/// a value into a key or a registry payload.
fn config_scalar(value: &JsonValue) -> Option<String> {
    match value {
        JsonValue::String(string) => Some(string.clone()),
        JsonValue::Bool(boolean) => Some(if *boolean {
            "1".to_owned()
        } else {
            "0".to_owned()
        }),
        JsonValue::Number(number) => number.as_u64().map(|number| number.to_string()),
        _ => None,
    }
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
            if let Some(value) = config.get(&name).and_then(config_scalar) {
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

/// Relative name an archive member will be written to, after
/// sanitisation and the optional proxy-DLL rename.
///
/// A recipe that named its payload DLL (`payload` / `payloadField`)
/// gets exactly that member renamed. A recipe that named none falls
/// back to the names this step shipped with, so ReShade-era chains keep
/// behaving the way they always have.
///
/// Shared by the executor and by [`plan_step_targets`] so the files a
/// step is about to overwrite are exactly the files the transaction
/// backs up first.
fn archive_member_target(safe_name: &str, proxy: Option<&str>, payload: Option<&str>) -> String {
    if let Some(proxy_name) = proxy.filter(|name| !name.is_empty()) {
        let basename = Path::new(safe_name)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if payload == Some(safe_name) {
            return proxy_name.to_owned();
        }
        if payload.is_none() && matches!(basename.as_str(), "reshade64.dll" | "dxgi.dll") {
            return proxy_name.to_owned();
        }
    }
    safe_name.to_owned()
}

/// Resolve a step's *write* target, refusing to escape its directory.
///
/// Step paths are templates rendered from config, so a `..` inside a
/// config value or a recipe would aim anywhere on the machine. That is
/// refused outright — no step legitimately needs it, so there is no
/// opt-in and no escape hatch.
///
/// An absolute path is a different case and stays allowed. Config
/// fields of type `path` exist precisely to name a location outside the
/// game folder (an OpenXR runtime manifest, a tray INI beside the
/// executable), and a recipe author writes that value on purpose. The
/// transaction store separately refuses to *back up* anything outside
/// the install root, and the runner reports those paths as not covered
/// by Undo.
fn resolve_write_target(step: &StepSpec, base: &Path, candidate: &str) -> Result<PathBuf, String> {
    let path = Path::new(candidate);
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }

    // Reject `..` that walks out of `base`, while still allowing
    // normalised inner paths such as `BepInEx/core/../winhttp.dll`.
    let mut depth: i64 = 0;
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                depth -= 1;
                if depth < 0 {
                    return Err(format!(
                        "{}: path '{}' escapes the install directory.",
                        step.kind, candidate
                    ));
                }
            }
            std::path::Component::Normal(_) => depth += 1,
            std::path::Component::CurDir => {}
            std::path::Component::RootDir | std::path::Component::Prefix(_) => {}
        }
    }

    Ok(base.join(path))
}

/// Filesystem paths a step is about to create, modify or remove,
/// resolved to absolute paths.
///
/// The runner calls this *before* the step runs and hands the result to
/// the transaction store, so a file the step is about to overwrite is
/// backed up in its original state and a step that fails halfway still
/// leaves a rollback record behind. Steps that write outside the
/// install directory are returned too; the runner filters them out of
/// the rollback set but keeps them for disclosure.
pub fn plan_step_targets(
    step: &StepSpec,
    context: &StepContext<'_>,
) -> Result<Vec<PathBuf>, String> {
    let mut targets: Vec<PathBuf> = Vec::new();
    let mut add = |candidate: &str| -> Result<(), String> {
        let path = resolve_write_target(step, context.executable_directory, candidate)?;
        if !targets.contains(&path) {
            targets.push(path);
        }
        Ok(())
    };

    match step.kind.as_str() {
        "download-file" => {
            // A bare filename lands in Moddin's own cache, which is not
            // part of the game's rollback scope. Anything else is written
            // next to the game and must be covered.
            if let Some(target) = param_string(step, "targetField")
                .and_then(|field| context.config.get_string(field))
                .or_else(|| param_string(step, "target").map(str::to_owned))
            {
                let path = Path::new(&target);
                let is_bare = path.components().count() == 1 && path.file_name().is_some();
                if !is_bare {
                    add(&target)?;
                }
            }
        }
        "extract-zip" => {
            let Some(archive_path) = param_string(step, "archivePathField")
                .and_then(|field| context.config.get_string(field))
                .or_else(|| param_string(step, "archivePath").map(str::to_owned))
            else {
                return Err(
                    "extract-zip: step needs archiveBytesField, archivePathField or archivePath."
                        .to_owned(),
                );
            };
            let resolved = resolve_download_path(context.executable_directory, &archive_path);
            if !resolved.is_file() {
                // The previous `download-file` step has not run yet or
                // failed. Returning no targets keeps the transaction
                // accurate; the step itself will report the real error.
                return Ok(targets);
            }
            let staging_root = resolve_staging_root(step, context)?;
            let bytes = fs::read(&resolved).map_err(|error| {
                format!(
                    "extract-zip: could not read archive '{}': {error}",
                    resolved.display()
                )
            })?;
            let Some(format) = detect_archive_format(&bytes) else {
                return Err(format!(
                    "extract-zip: could not open zip: '{archive_path}' is not a zip or a 7z \
                     archive (it looks like {}). Only zip and 7z payloads can be extracted.",
                    describe_archive_format(&bytes)
                ));
            };
            let names = archive_member_names(&bytes, format)?;
            let proxy =
                param_string(step, "proxyField").and_then(|field| context.config.get_string(field));
            let payload = declared_payload_member(step, context, &names)?;
            for member in archive_member_targets(&names, proxy.as_deref(), payload.as_deref())? {
                let target = staging_root.join(&member.target);
                if !targets.contains(&target) {
                    targets.push(target);
                }
            }
        }
        "build-project" => {
            // The build is the step that actually writes into the game,
            // so the paths it declares are what the transaction has to
            // cover. A working directory that does not exist yet has
            // nothing to plan; the step itself reports the real error.
            let Ok(directory) = build_working_directory(step, context) else {
                return Ok(targets);
            };
            for output in declared_outputs(step, context.config).unwrap_or_default() {
                let target = resolve_write_target(step, &directory, &output)?;
                if !targets.contains(&target) {
                    targets.push(target);
                }
            }
        }
        "file-delete" | "write-text-file" | "write-binary-file" => {
            // A step whose params are missing plans nothing; the step
            // itself reports the real error a moment later.
            if let Ok(path) = path_param(step, context.config, "pathField", "path") {
                add(&path)?;
            }
        }
        "move-file" => {
            // Both ends matter: `from` is restored if the move clobbered
            // it, and `to` is restored to its previous content.
            if let Ok(from) = path_param(step, context.config, "fromField", "from") {
                add(&from)?;
            }
            if let Ok(to) = path_param(step, context.config, "toField", "to") {
                add(&to)?;
            }
        }
        // verify-hash and kill-process are read-only; spawn-process does
        // not modify the executable; registry-write and registry-delete
        // do not touch the filesystem at all. git-checkout is
        // deliberately unplanned: a source tree is a working directory
        // in Moddin's own tools folder, not game content, and backing up
        // a whole repository on every install would be pure cost.
        _ => {}
    }

    Ok(targets)
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

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::{BTreeMap, BTreeSet};

    #[test]
    fn known_kinds_include_the_core_set() {
        let kinds = known_kinds();
        assert!(kinds.contains(&"extract-zip"));
        assert!(kinds.contains(&"git-checkout"));
        assert!(kinds.contains(&"build-project"));
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
        config
            .values
            .insert("backend".to_owned(), json!("fidelityfx"));
        config
            .values
            .insert("nvidia_preset".to_owned(), json!("medium"));
        let rendered = render_template(
            "[tray]\nbackend={backend}\nnvidia_preset={nvidia_preset}\n",
            &config,
        );
        assert_eq!(
            rendered,
            "[tray]\nbackend=fidelityfx\nnvidia_preset=medium\n"
        );
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

    /// A `StepContext` carrying the config and a scratch root. `StepContext`
    /// no longer takes a `CapabilitySpec`, so the steps under test no longer
    /// need one synthesised.
    fn download_context<'a>(config: &'a ResolvedConfig, root: &'a Path) -> StepContext<'a> {
        StepContext {
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
        assert!(
            missing_url.contains("urlField or url is required"),
            "{missing_url}"
        );

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
        config
            .values
            .insert("downloadUrl".to_owned(), json!("http://example.com/x"));
        config.values.insert("sha256".to_owned(), json!("deadbeef"));

        // The URL comes from config, so the scheme check must still run:
        // a recipe cannot smuggle plain HTTP in through a field.
        let error = execute_step(
            &download_step(&[
                ("urlField", json!("downloadUrl")),
                ("target", json!("out.zip")),
            ]),
            &download_context(&config, &root),
        )
        .expect_err("config-sourced URLs are validated too");
        assert!(error.contains("only HTTPS"), "{error}");

        // A missing field is a clear error, not a silent default.
        let error = execute_step(
            &download_step(&[
                ("urlField", json!("absentField")),
                ("target", json!("out.zip")),
            ]),
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

    // -- new step kinds -------------------------------------------------

    /// A throwaway directory under the temp root, removed when the guard
    /// drops. Unique per test so the suite can run in parallel.
    struct TempTree(PathBuf);

    impl TempTree {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "moddin-steps-{name}-{}",
                uuid::Uuid::new_v4().simple()
            ));
            fs::create_dir_all(&path).expect("temporary test directory");
            Self(path)
        }
    }

    impl Drop for TempTree {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// A real zip on disk. The extract tests need bytes the parser
    /// accepts, not the placeholder the literal-path test uses to prove
    /// a path was resolved.
    fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let file = File::create(path).expect("create archive");
        let mut writer = zip::ZipWriter::new(file);
        for (name, body) in entries {
            let options = zip::write::FileOptions::<()>::default()
                .compression_method(zip::CompressionMethod::Stored);
            writer.start_file(*name, options).expect("start entry");
            writer.write_all(body).expect("write entry");
        }
        writer.finish().expect("finish archive");
    }

    fn step_of(kind: &str, params: &[(&str, JsonValue)]) -> StepSpec {
        StepSpec {
            kind: kind.to_owned(),
            params: params
                .iter()
                .map(|(key, value)| ((*key).to_owned(), value.clone()))
                .collect(),
            description: None,
        }
    }

    #[test]
    fn extract_zip_writes_into_a_declared_staging_subdirectory() {
        let tree = TempTree::new("extract-subdir");
        let root = tree.0.clone();
        let archive = root.join("payload.zip");
        write_zip(&archive, &[("ofxr/manifest.json", b"{}")]);

        let spec = step_of(
            "extract-zip",
            &[
                ("archivePath", json!(archive.to_string_lossy())),
                ("targetSubdir", json!("staging")),
            ],
        );
        let config = ResolvedConfig::default();

        let result = execute_step(&spec, &download_context(&config, &root))
            .expect("the archive extracts into the staging directory");
        assert_eq!(
            fs::read_to_string(root.join("staging").join("ofxr").join("manifest.json"))
                .expect("staged member"),
            "{}"
        );
        assert!(
            !root.join("ofxr").exists(),
            "the member must not also land in the game directory"
        );
        assert_eq!(
            result.affected_paths,
            vec!["staging/ofxr/manifest.json".to_owned()]
        );

        // The transaction has to be handed the path the step really
        // wrote, or a file already sitting there is overwritten with no
        // backup behind it.
        assert_eq!(
            plan_step_targets(&spec, &download_context(&config, &root)).expect("planned targets"),
            vec![root.join("staging").join("ofxr").join("manifest.json")]
        );
    }

    #[test]
    fn extract_zip_without_a_subdirectory_still_lands_in_the_game_directory() {
        let tree = TempTree::new("extract-default");
        let root = tree.0.clone();
        let archive = root.join("payload.zip");
        write_zip(&archive, &[("ofxr/manifest.json", b"{}")]);

        let spec = step_of(
            "extract-zip",
            &[("archivePath", json!(archive.to_string_lossy()))],
        );
        let config = ResolvedConfig::default();

        let result = execute_step(&spec, &download_context(&config, &root))
            .expect("the archive extracts next to the executable");
        assert!(root.join("ofxr").join("manifest.json").is_file());
        assert_eq!(result.affected_paths, vec!["ofxr/manifest.json".to_owned()]);
    }

    #[test]
    fn extract_zip_refuses_a_staging_directory_that_escapes_the_game_folder() {
        let tree = TempTree::new("extract-escape");
        let root = tree.0.clone();
        let archive = root.join("payload.zip");
        write_zip(&archive, &[("manifest.json", b"{}")]);

        let spec = step_of(
            "extract-zip",
            &[
                ("archivePath", json!(archive.to_string_lossy())),
                ("targetSubdir", json!("../escaped")),
            ],
        );
        let config = ResolvedConfig::default();

        let error = execute_step(&spec, &download_context(&config, &root))
            .expect_err("a staging directory outside the game folder is refused");
        assert!(error.contains("escapes the install directory"), "{error}");
    }

    /// A real 7z payload, built with the same crate the step reads it
    /// with, so the fixture is a real archive rather than a byte string
    /// that only looks like one.
    fn write_7z(path: &Path, entries: &[(&str, &[u8])]) {
        let mut writer =
            sevenz_rust::SevenZWriter::new(Cursor::new(Vec::new())).expect("seven-z writer");
        for (name, body) in entries {
            // The struct has a private field, so it is built through
            // `new()` rather than struct-update syntax.
            let mut entry = sevenz_rust::SevenZArchiveEntry::new();
            entry.name = (*name).to_owned();
            writer
                .push_archive_entry(entry, Some(*body))
                .expect("push seven-z entry");
        }
        let finished = writer.finish().expect("finish seven-z archive");
        fs::write(path, finished.into_inner()).expect("write seven-z archive");
    }

    #[test]
    fn extract_step_opens_a_seven_zip_whatever_the_staging_file_is_called() {
        let tree = TempTree::new("extract-7z");
        let root = tree.0.clone();
        // Upstream OptiScaler publishes exactly one asset and it is a
        // 7z. The staging name is the recipe author's choice and lies
        // about the content, so the step has to read the bytes.
        let archive = root.join("optiscaler.zip");
        write_7z(
            &archive,
            &[
                ("OptiScaler.dll", b"the payload"),
                ("OptiScaler.ini", b"[Upscaler]\n"),
            ],
        );

        let spec = step_of(
            "extract-zip",
            &[("archivePath", json!(archive.to_string_lossy()))],
        );
        let config = ResolvedConfig::default();

        let result = execute_step(&spec, &download_context(&config, &root))
            .expect("a 7z payload extracts even when the file is named .zip");
        assert_eq!(
            fs::read(root.join("OptiScaler.dll")).expect("payload"),
            b"the payload"
        );
        assert_eq!(
            fs::read_to_string(root.join("OptiScaler.ini")).expect("ini"),
            "[Upscaler]\n"
        );
        assert_eq!(
            result.affected_paths,
            vec!["OptiScaler.dll".to_owned(), "OptiScaler.ini".to_owned()]
        );
    }

    #[test]
    fn extract_step_refuses_an_archive_it_cannot_name() {
        let tree = TempTree::new("extract-unknown");
        let root = tree.0.clone();
        let archive = root.join("payload.zip");
        // gzip: a real and common container this step deliberately has
        // no decoder for. It must be refused by name, not extracted.
        fs::write(&archive, b"\x1f\x8b\x08\x00payload").expect("write gzip-shaped bytes");

        let spec = step_of(
            "extract-zip",
            &[("archivePath", json!(archive.to_string_lossy()))],
        );
        let config = ResolvedConfig::default();

        let error = execute_step(&spec, &download_context(&config, &root))
            .expect_err("a format the step cannot read is refused, not extracted");
        assert!(
            error.contains("gzip"),
            "the error names the format: {error}"
        );
        assert!(
            !root.join("payload").exists(),
            "nothing may be written for an unreadable archive: {error}"
        );
    }

    #[test]
    fn the_recipe_names_its_payload_dll_and_it_takes_the_proxy_name() {
        let tree = TempTree::new("extract-payload");
        let root = tree.0.clone();
        let archive = root.join("optiscaler.7z");
        write_7z(
            &archive,
            &[
                ("OptiScaler.dll", b"the payload"),
                ("OptiScaler.ini", b"[Upscaler]\n"),
                ("extras/docs/OptiScaler.dll", b"a bundled copy"),
            ],
        );

        let mut config = ResolvedConfig::default();
        config.values.insert("proxy".to_owned(), json!("dxgi.dll"));
        config
            .values
            .insert("payloadDll".to_owned(), json!("OptiScaler.dll"));
        let spec = step_of(
            "extract-zip",
            &[
                ("archivePath", json!(archive.to_string_lossy())),
                ("proxyField", json!("proxy")),
                ("payloadField", json!("payloadDll")),
            ],
        );

        execute_step(&spec, &download_context(&config, &root)).expect("the archive installs");

        // The shallowest copy is the payload, which is the same rule the
        // dedicated OptiScaler module applies in `find_payload_root`.
        assert_eq!(
            fs::read(root.join("dxgi.dll")).expect("proxy"),
            b"the payload"
        );
        assert!(
            !root.join("OptiScaler.dll").exists(),
            "the payload is renamed, not copied"
        );
        assert_eq!(
            fs::read(root.join("extras").join("docs").join("OptiScaler.dll"))
                .expect("the deeper copy keeps its own name"),
            b"a bundled copy"
        );
        assert_eq!(
            fs::read_to_string(root.join("OptiScaler.ini")).expect("ini"),
            "[Upscaler]\n"
        );
    }

    #[test]
    fn a_declared_payload_the_archive_does_not_contain_stops_the_install() {
        let tree = TempTree::new("extract-payload-missing");
        let root = tree.0.clone();
        let archive = root.join("optiscaler.7z");
        write_7z(&archive, &[("OptiScaler.ini", b"[Upscaler]\n")]);

        let mut config = ResolvedConfig::default();
        config.values.insert("proxy".to_owned(), json!("dxgi.dll"));
        config
            .values
            .insert("payloadDll".to_owned(), json!("OptiScaler.dll"));
        let spec = step_of(
            "extract-zip",
            &[
                ("archivePath", json!(archive.to_string_lossy())),
                ("proxyField", json!("proxy")),
                ("payloadField", json!("payloadDll")),
            ],
        );

        let error = execute_step(&spec, &download_context(&config, &root))
            .expect_err("a payload the archive does not ship is not a success");
        assert!(error.contains("OptiScaler.dll"), "{error}");
        assert!(
            !root.join("dxgi.dll").exists(),
            "no proxy may be invented: {error}"
        );
    }

    #[test]
    fn the_transaction_plans_the_same_paths_a_seven_zip_writes() {
        let tree = TempTree::new("extract-7z-plan");
        let root = tree.0.clone();
        let archive = root.join("optiscaler.7z");
        write_7z(
            &archive,
            &[
                ("OptiScaler.dll", b"the payload"),
                ("notes/readme.txt", b"hi"),
            ],
        );

        let mut config = ResolvedConfig::default();
        config.values.insert("proxy".to_owned(), json!("dxgi.dll"));
        config
            .values
            .insert("payloadDll".to_owned(), json!("OptiScaler.dll"));
        let spec = step_of(
            "extract-zip",
            &[
                ("archivePath", json!(archive.to_string_lossy())),
                ("proxyField", json!("proxy")),
                ("payloadField", json!("payloadDll")),
            ],
        );

        let mut planned =
            plan_step_targets(&spec, &download_context(&config, &root)).expect("planned targets");
        planned.sort();
        assert_eq!(
            planned,
            vec![root.join("dxgi.dll"), root.join("notes").join("readme.txt")]
        );
    }

    #[test]
    fn a_recipe_that_names_no_payload_keeps_the_reshade_proxy_rule() {
        let tree = TempTree::new("extract-default-payload");
        let root = tree.0.clone();
        let archive = root.join("payload.zip");
        write_zip(
            &archive,
            &[
                ("ReShade/ReShade64.dll", b"reshade"),
                ("ReShade/ReShade.ini", b"[General]\n"),
            ],
        );

        let mut config = ResolvedConfig::default();
        config
            .values
            .insert("proxy".to_owned(), json!("winhttp.dll"));
        let spec = step_of(
            "extract-zip",
            &[
                ("archivePath", json!(archive.to_string_lossy())),
                ("proxyField", json!("proxy")),
            ],
        );

        execute_step(&spec, &download_context(&config, &root)).expect("the archive installs");
        assert!(
            !root.join("ReShade").join("ReShade64.dll").exists(),
            "the payload still takes the proxy name"
        );
        assert_eq!(
            fs::read(root.join("winhttp.dll")).expect("the renamed payload"),
            b"reshade"
        );
        assert_eq!(
            fs::read(root.join("ReShade").join("ReShade.ini")).expect("sibling kept its name"),
            b"[General]\n"
        );
    }

    #[test]
    fn the_container_is_chosen_by_its_bytes_not_by_its_name() {
        assert_eq!(
            detect_archive_format(b"PK\x03\x04rest"),
            Some(ArchiveFormat::Zip)
        );
        assert_eq!(
            detect_archive_format(b"PK\x05\x06"),
            Some(ArchiveFormat::Zip)
        );
        assert_eq!(
            detect_archive_format(b"7z\xBC\xAF\x27\x1Crest"),
            Some(ArchiveFormat::SevenZ)
        );
        // The extension is the recipe author's guess; none of these are
        // a container this step can open, whatever they are called.
        assert_eq!(detect_archive_format(b"not an archive"), None);
        assert_eq!(detect_archive_format(b""), None);
        assert_eq!(describe_archive_format(b"\x1f\x8b\x08"), "gzip");
        assert_eq!(describe_archive_format(b"Rar!\x1a\x07\x00"), "rar");
        assert_eq!(describe_archive_format(b"Rar!\x1a\x07\x01\x00"), "rar5");
        assert!(describe_archive_format(b"tarball").contains("74 61 72"));
        assert_eq!(describe_archive_format(b""), "an empty file");
        // tar has no leading magic; its identifier sits at offset 257.
        let mut tar = vec![0u8; 257];
        tar.extend_from_slice(b"ustar");
        assert_eq!(describe_archive_format(&tar), "tar");
    }

    #[test]
    fn a_directory_move_promotes_the_whole_staged_tree() {
        let tree = TempTree::new("move-directory");
        let root = tree.0.clone();
        fs::create_dir_all(root.join("staging").join("ofxr")).expect("staged tree");
        fs::write(root.join("staging").join("ofxr").join("layer.dll"), b"dll")
            .expect("staged file");

        let spec = step_of(
            "move-file",
            &[("from", json!("staging")), ("to", json!("ApiLayers"))],
        );
        let config = ResolvedConfig::default();

        let result = execute_step(&spec, &download_context(&config, &root))
            .expect("the tree moves as one rename");
        assert!(!root.join("staging").exists(), "the source is gone");
        assert!(root
            .join("ApiLayers")
            .join("ofxr")
            .join("layer.dll")
            .is_file());
        assert_eq!(
            result.affected_paths.len(),
            2,
            "{:?}",
            result.affected_paths
        );
    }

    #[test]
    fn a_directory_move_refuses_a_destination_that_already_exists() {
        let tree = TempTree::new("move-occupied");
        let root = tree.0.clone();
        fs::create_dir_all(root.join("staging")).expect("staged tree");
        fs::write(root.join("staging").join("layer.dll"), b"dll").expect("staged file");

        // The runtime already owns this folder, and the transaction
        // store cannot put a directory back — so the move has to stop
        // rather than replace it.
        let destination = root.join("ApiLayers");
        fs::create_dir_all(&destination).expect("occupied destination");
        fs::write(destination.join("owned.json"), b"{}").expect("pre-existing file");

        let spec = step_of(
            "move-file",
            &[("from", json!("staging")), ("to", json!("ApiLayers"))],
        );
        let config = ResolvedConfig::default();

        let error = execute_step(&spec, &download_context(&config, &root))
            .expect_err("an occupied destination is refused");
        assert!(error.contains("already exists"), "{error}");
        assert!(
            root.join("staging").is_dir(),
            "the staged tree is untouched"
        );
        assert!(
            destination.join("owned.json").is_file(),
            "the destination is untouched"
        );
    }

    #[test]
    fn move_file_still_refuses_a_source_equal_to_its_destination() {
        let tree = TempTree::new("move-onto-itself");
        let root = tree.0.clone();
        let spec = step_of(
            "move-file",
            &[("from", json!("same.txt")), ("to", json!("same.txt"))],
        );
        let config = ResolvedConfig::default();

        let error = execute_step(&spec, &download_context(&config, &root))
            .expect_err("moving a file onto itself is refused");
        assert!(error.contains("same path"), "{error}");
    }

    #[test]
    fn path_params_render_config_fields() {
        let mut config = ResolvedConfig::default();
        config
            .values
            .insert("extractedDir".to_owned(), json!("C:/tools/ofxr"));
        let spec = step_of(
            "move-file",
            &[("from", json!("{extractedDir}/XR_APILAYER_manual.json"))],
        );
        assert_eq!(
            path_param(&spec, &config, "fromField", "from").expect("rendered"),
            "C:/tools/ofxr/XR_APILAYER_manual.json",
            "a recipe can name a file inside the directory it staged into"
        );
    }

    #[test]
    fn git_checkout_requires_a_repository_a_ref_and_a_target() {
        let tree = TempTree::new("git-required");
        let root = tree.0.clone();
        let config = ResolvedConfig::default();

        let error = execute_step(
            &step_of(
                "git-checkout",
                &[("ref", json!("v1.8")), ("target", json!("src"))],
            ),
            &download_context(&config, &root),
        )
        .expect_err("no repository is rejected");
        assert!(error.contains("repoField or repo is required"), "{error}");

        let error = execute_step(
            &step_of(
                "git-checkout",
                &[
                    ("repo", json!("https://github.com/praydog/UEVR")),
                    ("target", json!("src")),
                ],
            ),
            &download_context(&config, &root),
        )
        .expect_err("no ref is rejected");
        assert!(error.contains("refField or ref is required"), "{error}");

        let error = execute_step(
            &step_of(
                "git-checkout",
                &[
                    ("repo", json!("https://github.com/praydog/UEVR")),
                    ("ref", json!("v1.8")),
                ],
            ),
            &download_context(&config, &root),
        )
        .expect_err("no target is rejected");
        assert!(
            error.contains("targetField or target is required"),
            "{error}"
        );
    }

    #[test]
    fn git_checkout_refuses_plain_http() {
        let tree = TempTree::new("git-http");
        let root = tree.0.clone();
        let config = ResolvedConfig::default();
        let spec = step_of(
            "git-checkout",
            &[
                ("repo", json!("http://github.com/praydog/UEVR")),
                ("ref", json!("v1.8")),
                ("target", json!("uevr-src")),
            ],
        );
        let error = execute_step(&spec, &download_context(&config, &root))
            .expect_err("plain HTTP is refused");
        assert!(error.contains("only HTTPS"), "{error}");
    }

    #[test]
    fn git_checkout_refuses_embedded_credentials() {
        let tree = TempTree::new("git-credentials");
        let root = tree.0.clone();
        let config = ResolvedConfig::default();
        let spec = step_of(
            "git-checkout",
            &[
                ("repo", json!("https://user:secret@github.com/praydog/UEVR")),
                ("ref", json!("v1.8")),
                ("target", json!("uevr-src")),
            ],
        );
        let error = execute_step(&spec, &download_context(&config, &root))
            .expect_err("credentials are refused");
        assert!(error.contains("credentials"), "{error}");
    }

    #[test]
    fn git_checkout_refuses_hosts_outside_the_allowlist() {
        let tree = TempTree::new("git-host");
        let root = tree.0.clone();
        let config = ResolvedConfig::default();
        let spec = step_of(
            "git-checkout",
            &[
                ("repo", json!("https://evil.example.com/UEVR.git")),
                ("ref", json!("v1.8")),
                ("target", json!("uevr-src")),
            ],
        );
        let error = execute_step(&spec, &download_context(&config, &root))
            .expect_err("an unlisted host is refused");
        assert!(error.contains("not in the allow-list"), "{error}");
        assert!(
            error.contains("github.com"),
            "the error lists what is allowed: {error}"
        );
    }

    #[test]
    fn git_checkout_refuses_a_branch_name() {
        let tree = TempTree::new("git-branch");
        let root = tree.0.clone();
        let config = ResolvedConfig::default();

        for branch in ["refs/heads/main", "main-branch", "HEAD"] {
            let spec = step_of(
                "git-checkout",
                &[
                    ("repo", json!("https://github.com/praydog/UEVR")),
                    ("ref", json!(branch)),
                    ("target", json!("uevr-src")),
                ],
            );
            let error = execute_step(&spec, &download_context(&config, &root)).unwrap_err();
            assert!(
                !error.contains("could not run"),
                "{branch} was not rejected: {error}"
            );
        }

        let error = execute_step(
            &step_of(
                "git-checkout",
                &[
                    ("repo", json!("https://github.com/praydog/UEVR")),
                    ("ref", json!("refs/heads/main")),
                    ("target", json!("uevr-src")),
                ],
            ),
            &download_context(&config, &root),
        )
        .expect_err("a branch is refused");
        assert!(error.contains("is a branch"), "{error}");
    }

    #[test]
    fn git_checkout_pins_an_exact_tag_or_a_full_commit() {
        let tag = classify_pinned_ref("v1.8").expect("a plain tag");
        assert_eq!(tag, PinnedRef::Tag("v1.8".to_owned()));
        assert_eq!(
            classify_pinned_ref("refs/tags/v1.8").expect("a fully qualified tag"),
            PinnedRef::Tag("v1.8".to_owned()),
            "the refs/tags/ prefix is stripped so the fetch is namespaced once"
        );
        assert_eq!(
            classify_pinned_ref("1.0.0-rc1").expect("a prerelease tag"),
            PinnedRef::Tag("1.0.0-rc1".to_owned())
        );

        let sha = "0123456789abcdef0123456789abcdef01234567";
        assert_eq!(
            classify_pinned_ref(sha).expect("a full commit"),
            PinnedRef::Commit(sha.to_owned())
        );

        // A short SHA is the trap: two commits can share seven
        // characters, so the install would stop reproducing.
        let error = classify_pinned_ref(&sha[..8]).expect_err("a short commit is refused");
        assert!(error.contains("abbreviated commit"), "{error}");

        assert!(classify_pinned_ref("").is_err(), "an empty ref is refused");
        assert!(
            classify_pinned_ref("feature/whatever").is_err(),
            "an unqualified name with a slash could be a branch path"
        );
        assert!(
            classify_pinned_ref("v1..8").is_err(),
            "git refuses a ref containing '..'"
        );
    }

    #[test]
    fn build_project_refuses_a_directory_it_cannot_run_in() {
        let tree = TempTree::new("build-no-directory");
        let root = tree.0.clone();
        let config = ResolvedConfig::default();
        let spec = step_of(
            "build-project",
            &[
                ("directory", json!("uevr-src")),
                ("command", json!("build-release.bat")),
                ("outputs", json!(["injector/UEVRInjector.exe"])),
            ],
        );
        let error = execute_step(&spec, &download_context(&config, &root))
            .expect_err("a missing checkout is refused");
        assert!(error.contains("is not a directory"), "{error}");
    }

    #[test]
    fn build_project_requires_declared_outputs() {
        let tree = TempTree::new("build-no-outputs");
        let root = tree.0.clone();
        fs::create_dir_all(root.join("src")).expect("checkout");
        let config = ResolvedConfig::default();

        let spec = step_of(
            "build-project",
            &[
                ("directory", json!("src")),
                ("command", json!("nothing.bat")),
            ],
        );
        let error = execute_step(&spec, &download_context(&config, &root))
            .expect_err("a build with no declared output is refused");
        assert!(error.contains("outputs is required"), "{error}");

        let spec = step_of(
            "build-project",
            &[
                ("directory", json!("src")),
                ("command", json!("nothing.bat")),
                ("outputs", json!([])),
            ],
        );
        let error = execute_step(&spec, &download_context(&config, &root))
            .expect_err("an empty output list is refused");
        assert!(error.contains("outputs is required"), "{error}");
    }

    #[test]
    fn build_project_runs_the_command_and_checks_what_it_declared() {
        let tree = TempTree::new("build-run");
        let root = tree.0.clone();
        fs::create_dir_all(root.join("src")).expect("checkout");
        let config = ResolvedConfig::default();

        // `cmd` stands in for a project's build script: it is the one
        // tool guaranteed to be on PATH, and it writes into the
        // working directory, which is what the step has to set.
        let spec = step_of(
            "build-project",
            &[
                ("directory", json!("src")),
                ("command", json!("cmd")),
                ("args", json!(["/c", "echo built>artifact.bin"])),
                ("outputs", json!(["artifact.bin"])),
            ],
        );
        let result = execute_step(&spec, &download_context(&config, &root))
            .expect("the build runs and produces its declared output");
        assert_eq!(
            result.affected_paths,
            vec![root
                .join("src")
                .join("artifact.bin")
                .to_string_lossy()
                .into_owned()],
            "the transaction is told what the build produced"
        );
        assert!(
            root.join("src").join("artifact.bin").is_file(),
            "the build ran in the checkout, not somewhere else"
        );

        // A build that exits cleanly but writes nothing the recipe named
        // has not succeeded, and saying so beats a silent no-op mod.
        let spec = step_of(
            "build-project",
            &[
                ("directory", json!("src")),
                ("command", json!("cmd")),
                ("args", json!(["/c", "ver>nul"])),
                ("outputs", json!(["never-written.bin"])),
            ],
        );
        let error = execute_step(&spec, &download_context(&config, &root))
            .expect_err("a missing declared output fails the step");
        assert!(error.contains("did not produce"), "{error}");
    }

    #[test]
    fn build_project_takes_its_command_from_a_config_field() {
        let tree = TempTree::new("build-command-field");
        let root = tree.0.clone();
        fs::create_dir_all(root.join("src")).expect("checkout");
        let mut config = ResolvedConfig::default();
        config
            .values
            .insert("buildCommand".to_owned(), json!("cmd"));

        let spec = step_of(
            "build-project",
            &[
                ("directoryField", json!("sourceDir")),
                ("commandField", json!("buildCommand")),
                ("args", json!(["/c", "echo built>artifact.bin"])),
                ("outputs", json!(["artifact.bin"])),
            ],
        );
        config.values.insert(
            "sourceDir".to_owned(),
            json!(root.join("src").to_string_lossy()),
        );

        execute_step(&spec, &download_context(&config, &root))
            .expect("the command comes from the config field the recipe named");
        assert!(root.join("src").join("artifact.bin").is_file());
    }

    /// A throwaway HKCU key, removed when the guard drops.
    ///
    /// `reg.exe` is only ever pointed at a key this suite created, under
    /// `HKCU\Software\Moddin\BuiltinStepsTest\<uuid>`. Unique per test so
    /// the suite can run in parallel, and nothing outside it is touched.
    struct RegScratch(String);

    impl RegScratch {
        fn new() -> Self {
            Self(format!(
                "HKCU\\Software\\Moddin\\BuiltinStepsTest\\{}",
                uuid::Uuid::new_v4().simple()
            ))
        }

        fn child(&self, name: &str) -> String {
            format!("{}\\{name}", self.0)
        }
    }

    impl Drop for RegScratch {
        fn drop(&mut self) {
            let mut command = std::process::Command::new("reg.exe");
            crate::process::HideConsole::hide_console(&mut command);
            let _ = command.arg("delete").arg(&self.0).arg("/f").output();
        }
    }

    /// `reg query` output for a key / value, or `None` when reg.exe says
    /// it is not there.
    fn reg_query(key: &str, value: Option<&str>) -> Option<String> {
        let mut command = std::process::Command::new("reg.exe");
        crate::process::HideConsole::hide_console(&mut command);
        command.arg("query").arg(key);
        if let Some(name) = value {
            command.arg("/v").arg(name);
        }
        let output = command.output().expect("reg.exe query");
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
    }

    #[test]
    fn registry_write_stores_a_string_value() {
        let scratch = RegScratch::new();
        let key = scratch.child("OpenXR");
        let config = ResolvedConfig::default();
        // Registry steps read neither directory, so any root will do.
        let root = std::env::temp_dir();
        let manifest = "C:\\Program Files\\Moddin\\runtime\\openxr_runtime.json";

        let spec = step_of(
            "registry-write",
            &[
                ("key", json!(key)),
                ("value", json!("ActiveRuntime")),
                ("type", json!("REG_SZ")),
                ("data", json!(manifest)),
            ],
        );
        let result = execute_step(&spec, &download_context(&config, &root))
            .expect("a REG_SZ value carrying data writes");
        assert_eq!(result.affected_paths, vec![format!("registry:{key}")]);

        let read_back = reg_query(&key, Some("ActiveRuntime")).expect("the value is there");
        assert!(read_back.contains(manifest), "{read_back}");
    }

    #[test]
    fn registry_write_stores_a_dword() {
        // The shape the step used to refuse outright: `reg add <key> /v
        // <name> /t REG_DWORD` with no /d answers "invalid syntax.
        // Specify a valid numeric value for '/d'".
        let scratch = RegScratch::new();
        let key = scratch.child("AvailableRuntimes");
        let config = ResolvedConfig::default();
        let root = std::env::temp_dir();

        let from_literal = step_of(
            "registry-write",
            &[
                ("key", json!(key)),
                ("value", json!("Enabled")),
                ("type", json!("REG_DWORD")),
                ("data", json!("1")),
            ],
        );
        execute_step(&from_literal, &download_context(&config, &root))
            .expect("a REG_DWORD carries its number as data");
        assert!(
            reg_query(&key, Some("Enabled"))
                .expect("the value is there")
                .contains("0x1"),
            "the number landed, not an empty value"
        );

        // A `number` config field is the natural source for a DWORD, so
        // `dataField` has to read one — not just a string field.
        let mut config = ResolvedConfig::default();
        config.values.insert("enabled".to_owned(), json!(2));
        let from_config = step_of(
            "registry-write",
            &[
                ("key", json!(key)),
                ("value", json!("Ordinal")),
                ("type", json!("REG_DWORD")),
                ("dataField", json!("enabled")),
            ],
        );
        execute_step(&from_config, &download_context(&config, &root))
            .expect("a REG_DWORD takes its number from a config field");
        assert!(
            reg_query(&key, Some("Ordinal"))
                .expect("the value is there")
                .contains("0x2"),
            "the number came from the field the recipe named"
        );
    }

    #[test]
    fn registry_write_refuses_a_dword_with_no_data() {
        let scratch = RegScratch::new();
        let key = scratch.child("AvailableRuntimes");
        let config = ResolvedConfig::default();
        let root = std::env::temp_dir();
        let spec = step_of(
            "registry-write",
            &[
                ("key", json!(key)),
                ("value", json!("Enabled")),
                ("type", json!("REG_DWORD")),
            ],
        );
        // reg.exe would answer this with a parse error about /d, which
        // tells a recipe author nothing about which param to set.
        let error = execute_step(&spec, &download_context(&config, &root))
            .expect_err("a DWORD with no number is not a writeable value");
        assert!(error.contains("set data or dataField"), "{error}");
    }

    #[test]
    fn registry_write_templates_a_per_game_key_from_a_config_field() {
        // A per-game key is always templated: the per-game name lives in
        // the key, so a recipe that spells it out writes one garbage key
        // for every game instead.
        let scratch = RegScratch::new();
        let root = std::env::temp_dir();
        let mut config = ResolvedConfig::default();
        config
            .values
            .insert("gameExecutable".to_owned(), json!("Cyberpunk2077.exe"));

        let spec = step_of(
            "registry-write",
            &[
                ("key", json!(scratch.child("per-game\\{gameExecutable}"))),
                ("value", json!("ActiveRuntime")),
                ("type", json!("REG_SZ")),
                ("dataField", json!("runtimeManifest")),
            ],
        );
        config
            .values
            .insert("runtimeManifest".to_owned(), json!("C:\\xr\\steamvr.json"));

        let result = execute_step(&spec, &download_context(&config, &root))
            .expect("the key renders from the config field");
        let per_game = scratch.child("per-game\\Cyberpunk2077.exe");
        assert_eq!(result.affected_paths, vec![format!("registry:{per_game}")]);
        assert!(
            reg_query(&per_game, Some("ActiveRuntime"))
                .expect("the value landed under the per-game key")
                .contains("C:\\xr\\steamvr.json"),
            "the manifest path is the value data, as the OpenXR runtime convention requires"
        );
        assert!(
            reg_query(&scratch.child("per-game\\{gameExecutable}"), None).is_none(),
            "the unrendered placeholder is not a key that got written"
        );
    }

    #[test]
    fn registry_write_without_data_still_writes_the_empty_value() {
        // The OpenXR implicit-layer convention: the value *name* is the
        // manifest path and the data is empty. That is what the step did
        // before it could carry data, and a recipe that declares no
        // `data` has to keep landing exactly there.
        let scratch = RegScratch::new();
        let key = scratch.child("ImplicitLayers");
        let config = ResolvedConfig::default();
        let root = std::env::temp_dir();
        let manifest = "C:\\xr\\implicit.json";

        let spec = step_of(
            "registry-write",
            &[
                ("key", json!(key)),
                ("value", json!(manifest)),
                ("type", json!("REG_SZ")),
            ],
        );
        execute_step(&spec, &download_context(&config, &root))
            .expect("a REG_SZ value with no data writes an empty value");

        let read_back = reg_query(&key, Some(manifest)).expect("the value is there");
        let line = read_back
            .lines()
            .find(|line| line.contains(manifest))
            .expect("the manifest-named value line");
        assert!(
            line.trim_end().ends_with("REG_SZ"),
            "the value still holds no data: {line:?}"
        );
    }

    #[test]
    fn registry_delete_is_idempotent() {
        // `reg delete` exits 1 both for "there was nothing to delete" and
        // for a real failure, so a step that forwarded that code made a
        // second uninstall fail on a machine that was already clean.
        let scratch = RegScratch::new();
        let key = scratch.child("per-game\\Cyberpunk2077.exe");
        let config = ResolvedConfig::default();
        let root = std::env::temp_dir();
        let remove = step_of(
            "registry-delete",
            &[
                ("key", json!(key)),
                ("value", json!("ActiveRuntime")),
                ("force", json!(true)),
            ],
        );

        execute_step(&remove, &download_context(&config, &root))
            .expect("deleting a value that was never written is a no-op");
        assert!(reg_query(&key, None).is_none(), "nothing to remove yet");

        let write = step_of(
            "registry-write",
            &[
                ("key", json!(key)),
                ("value", json!("ActiveRuntime")),
                ("type", json!("REG_SZ")),
                ("data", json!("C:\\xr\\steamvr.json")),
            ],
        );
        execute_step(&write, &download_context(&config, &root)).expect("the value is written");
        execute_step(&remove, &download_context(&config, &root))
            .expect("the first delete removes it");
        assert!(
            reg_query(&key, Some("ActiveRuntime")).is_none(),
            "the value is gone after the first delete"
        );
        execute_step(&remove, &download_context(&config, &root))
            .expect("the second delete has nothing left to do, so a chain can run twice");
    }

    /// The shipped OpenXR recipe, parsed by the type the runner loads it
    /// as, so a YAML edit that the loader would reject fails here first.
    const OPENXR_HELPERS_YAML: &str = include_str!("../capabilities/openxr-helpers.yaml");

    /// Every `{name}` a step param holds, the config fields named by a
    /// `*Field` param, and nothing else a step looks at.
    ///
    /// Shared with the recipe-wide invariant in `capability_runner`, which
    /// is about specs rather than about this module's steps.
    pub(crate) fn config_fields_a_step_reads(
        spec: &crate::capability::CapabilitySpec,
    ) -> BTreeSet<String> {
        let mut names = BTreeSet::new();
        for step in spec.install.iter().chain(spec.uninstall.iter()) {
            for (param, value) in &step.params {
                if param.ends_with("Field") {
                    if let Some(field) = value.as_str() {
                        names.insert(field.to_owned());
                    }
                    continue;
                }
                if let Some(text) = value.as_str() {
                    let mut rest = text;
                    while let Some(start) = rest.find('{') {
                        rest = &rest[start + 1..];
                        match rest.find('}') {
                            Some(end) => {
                                names.insert(rest[..end].to_owned());
                                rest = &rest[end + 1..];
                            }
                            None => break,
                        }
                    }
                }
            }
        }
        names
    }

    /// `openxr-helpers` is `planned` and declares no chain, because the
    /// OpenXR loader has no per-game registry key to write. The loader
    /// specification puts `ActiveRuntime` under one machine-level
    /// `HKLM\SOFTWARE\Khronos\OpenXR\<major_api_version>` value and states
    /// that "the selection of the active runtime is handled external to the
    /// loader". An earlier revision of this recipe wrote
    /// `HKCU\...\OpenXR\1\per-game\{gameExecutable}\ActiveRuntime` and
    /// described that as binding a runtime for this game alone: a key no
    /// OpenXR loader reads, verified against the specification on
    /// 2026-09-29 (registry.khronos.org/OpenXR/specs/1.1/loader.html).
    ///
    /// The test exists so that adding a chain back is a deliberate act with
    /// the reason attached, rather than a quiet edit. Making this recipe
    /// work is a runner gap — a step that expands environment variables to
    /// reach `%LOCALAPPDATA%\Moddin\profiles\openxr\`, and JSON escaping so
    /// a Windows path can be embedded in the preference document — not
    /// another step kind.
    #[test]
    fn openxr_helpers_is_planned_with_no_chain() {
        let spec: crate::capability::CapabilitySpec =
            serde_yaml::from_str(OPENXR_HELPERS_YAML).expect("openxr-helpers.yaml parses");

        assert_eq!(
            spec.status, "planned",
            "openxr-helpers is available again, so it can be installed. The OpenXR loader binds \
             ActiveRuntime from a single machine-level key and defines no per-game key; a chain \
             here writes a key nothing reads"
        );
        assert!(
            spec.install.is_empty() && spec.uninstall.is_empty(),
            "an empty chain is the honest shape for a planned recipe. A chain here describes an \
             effect no step can produce (install: {}, uninstall: {})",
            spec.install.len(),
            spec.uninstall.len()
        );
    }
}
