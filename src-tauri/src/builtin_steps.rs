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
    collections::{BTreeMap, BTreeSet, HashMap, VecDeque},
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
    // Both formats extract their members into the target root after
    // sanitising the member path, narrowed first by the step's
    // `include` / `exclude` filter when it declared one. Members that
    // match the recipe's declared payload DLL take the recipe's chosen
    // proxy name (only when `proxyField` resolves to a non-empty
    // string); a recipe that names no payload falls back to the
    // ReShade-era defaults.
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
    // The filter runs first, so `payload` and the proxy rename act on
    // the members this step is actually going to write.
    let selection = select_archive_members(step, &names)?;
    let payload = declared_payload_member(step, context, &selection.names)?;
    let members = archive_member_targets(&selection.names, proxy.as_deref(), payload.as_deref())?;

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
            // entry, in order, minus whatever the filter dropped — so one
            // cursor keeps the two aligned. A filtered-out member is
            // stepped over without consuming a planned member.
            let mut cursor = 0usize;
            let mut seen = 0usize;
            for index in 0..archive.len() {
                let mut entry = archive
                    .by_index(index)
                    .map_err(|error| format!("extract-zip: entry {index} unreadable: {error}"))?;
                if entry.is_dir() {
                    continue;
                }
                let name = entry.name().to_owned();
                let planned = match &selection.ordinals {
                    Some(ordinals) => ordinals.binary_search(&seen).is_ok(),
                    None => true,
                };
                seen += 1;
                if !planned {
                    continue;
                }
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
            let planned: BTreeSet<&str> = members
                .iter()
                .map(|member| member.safe_name.as_str())
                .collect();
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
                if !planned.contains(safe_name.as_str()) {
                    // With no filter every member is planned, so this is
                    // the reader disagreeing with the header and has to
                    // stop. With one it is a member the filter dropped,
                    // which is the step working as declared.
                    if selection.ordinals.is_none() {
                        stopped = Some(format!(
                            "extract-zip: the archive yielded a member '{name}' that was not planned."
                        ));
                        return Ok(false);
                    }
                    return Ok(true);
                }
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

/// The `include` / `exclude` member filter an `extract-zip` step
/// declared, or `None` when it declared neither.
///
/// An archive that ships a superset of what one game wants is the normal
/// case for an injected-DLL framework: REFramework v1.5.9.1 ships the
/// flat-screen injector and the VR payload together and its own release
/// note says a player with no headset must extract only `dinput8.dll`. A
/// step that writes every member cannot express that, and a workaround
/// that installs the wrong subset is worse than a refusal.
struct MemberFilter {
    /// Selects. Empty means "every member", so a recipe can declare
    /// `exclude` alone.
    include: Vec<String>,
    /// Removes from the selection.
    exclude: Vec<String>,
}

impl MemberFilter {
    /// Read the filter off a step. A param that is not an array of
    /// strings is an authoring error and is refused rather than ignored:
    /// a filter that silently did nothing would install the superset the
    /// recipe was written to avoid.
    fn from_step(step: &StepSpec) -> Result<Option<Self>, String> {
        let include = member_patterns(step, "include")?;
        let exclude = member_patterns(step, "exclude")?;
        if include.is_empty() && exclude.is_empty() {
            return Ok(None);
        }
        Ok(Some(Self { include, exclude }))
    }

    /// Does this member survive the filter? `name` is the member's path
    /// normalised to forward slashes.
    fn selects(&self, name: &str) -> bool {
        let included = self.include.is_empty()
            || self
                .include
                .iter()
                .any(|pattern| member_pattern_matches(pattern, name));
        included
            && !self
                .exclude
                .iter()
                .any(|pattern| member_pattern_matches(pattern, name))
    }
}

/// The string patterns a step declared under `name`, or an empty list
/// when it declared none.
fn member_patterns(step: &StepSpec, name: &str) -> Result<Vec<String>, String> {
    let Some(value) = param(step, name) else {
        return Ok(Vec::new());
    };
    let Some(values) = value.as_array() else {
        return Err(format!(
            "extract-zip: {name} is {value}, not an array of glob patterns."
        ));
    };
    let mut patterns = Vec::with_capacity(values.len());
    for (index, entry) in values.iter().enumerate() {
        let Some(pattern) = entry.as_str() else {
            return Err(format!(
                "extract-zip: {name}[{index}] is {entry}, not a string."
            ));
        };
        let pattern = pattern.trim();
        if pattern.is_empty() {
            return Err(format!("extract-zip: {name}[{index}] is empty."));
        }
        patterns.push(pattern.to_ascii_lowercase());
    }
    Ok(patterns)
}

/// Does one `include` / `exclude` pattern name this archive member?
///
/// - Matching is case-insensitive, because the filesystem the members
///   land on is.
/// - `*` matches any run of characters inside one path segment and `**`
///   matches any number of segments, so `*` does not cross a `/` and
///   `**` does.
/// - A pattern with no `/` in it is a basename and matches at any depth,
///   which is what makes `dinput8.dll` the obvious way to say "the
///   injector, wherever the archive puts it".
/// - Nothing else is a wildcard. A Windows member name cannot contain
///   `?`, `[` or `]`, so treating them as literals cannot silently
///   exclude a member the recipe meant to install.
fn member_pattern_matches(pattern: &str, normalised_name: &str) -> bool {
    let pattern = pattern.trim().to_ascii_lowercase();
    let name = normalised_name.to_ascii_lowercase();
    if pattern.contains('/') {
        let pattern_segments: Vec<&str> = pattern.split('/').collect();
        let name_segments: Vec<&str> = name.split('/').collect();
        segments_match(&pattern_segments, &name_segments)
    } else {
        match name.rsplit('/').next() {
            Some(basename) => segment_matches(&pattern, basename),
            None => false,
        }
    }
}

/// `**` in one segment position against the member's segments.
fn segments_match(pattern: &[&str], name: &[&str]) -> bool {
    let Some((head, rest)) = pattern.split_first() else {
        return name.is_empty();
    };
    if *head == "**" {
        // Any number of segments, including none: `reframework/**`
        // matches `reframework/autorun/mod/scripts/x.lua` and
        // `reframework/autorun` alike.
        return (0..=name.len()).any(|skip| segments_match(rest, &name[skip..]));
    }
    match name.split_first() {
        Some((first, tail)) if segment_matches(head, first) => segments_match(rest, tail),
        _ => false,
    }
}

/// One path segment, where `*` matches any run of characters.
///
/// Classic backtracking match: on a mismatch, the last `*` is stretched
/// by one character and the match resumes after it. Segments contain no
/// `/`, so a `*` here cannot escape its segment.
fn segment_matches(pattern: &str, name: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let name: Vec<char> = name.chars().collect();
    let (mut pattern_index, mut name_index) = (0usize, 0usize);
    let mut star: Option<(usize, usize)> = None;
    while name_index < name.len() {
        if pattern_index < pattern.len() && pattern[pattern_index] == '*' {
            star = Some((pattern_index, name_index));
            pattern_index += 1;
        } else if pattern_index < pattern.len() && pattern[pattern_index] == name[name_index] {
            pattern_index += 1;
            name_index += 1;
        } else if let Some((star_index, star_name)) = star {
            pattern_index = star_index + 1;
            name_index = star_name + 1;
            star = Some((star_index, star_name + 1));
        } else {
            return false;
        }
    }
    pattern[pattern_index..]
        .iter()
        .all(|character| *character == '*')
}

/// The members an `extract-zip` step will actually write, after its
/// `include` / `exclude` filter and in archive order.
struct MemberSelection {
    /// Sanitised member names, in archive order. When a filter is
    /// declared this is the filtered subset; otherwise it is every
    /// member, unchanged.
    names: Vec<String>,
    /// Ordinals into the archive walk, for the members the filter kept.
    /// `None` means every member is planned, so a reader that hands the
    /// members over in a different order (7z does) can look a member up
    /// by name instead.
    ordinals: Option<Vec<usize>>,
}

/// Apply the step's member filter to the archive's member list.
///
/// Shared by the executor and by [`plan_step_targets`] so the files a
/// step is about to write and the files the transaction is handed cannot
/// disagree about which members those are.
///
/// The filter runs before the payload lookup and the proxy rename, so
/// `payload` and `proxy` name members of the *filtered* set: narrowing
/// the archive cannot quietly turn the surviving member into a bundled
/// sample.
fn select_archive_members(step: &StepSpec, names: &[String]) -> Result<MemberSelection, String> {
    let Some(filter) = MemberFilter::from_step(step)? else {
        return Ok(MemberSelection {
            names: names.to_vec(),
            ordinals: None,
        });
    };

    let mut selection = MemberSelection {
        names: Vec::new(),
        ordinals: Some(Vec::new()),
    };
    for (ordinal, name) in names.iter().enumerate() {
        // Sanitised before it is matched, not after: a member that
        // walks out of the target root is refused outright, and a filter
        // must not be able to hide one by excluding it.
        let Some(safe_name) = sanitize_archive_member(name) else {
            return Err(format!("extract-zip: unsafe archive member '{name}'."));
        };
        if filter.selects(&safe_name) {
            selection.names.push(safe_name);
            if let Some(ordinals) = selection.ordinals.as_mut() {
                ordinals.push(ordinal);
            }
        }
    }

    if selection.names.is_empty() {
        // Refusing is the point. Extracting nothing and reporting
        // success is the worst outcome available: the mod is gone and
        // the user has been told it installed.
        return Err(format!(
            "extract-zip: the include/exclude filter selected none of the archive's {} members \
             (include: {}; exclude: {}). A filter that matches nothing is an authoring error or \
             an archive that has changed shape — a workaround that installs the wrong subset is \
             worse than a refusal.",
            names.len(),
            describe_patterns(&filter.include),
            describe_patterns(&filter.exclude),
        ));
    }
    Ok(selection)
}

/// A pattern list as it appears in a refusal message, so a recipe
/// author can see which of their patterns matched nothing.
fn describe_patterns(patterns: &[String]) -> String {
    if patterns.is_empty() {
        "none".to_owned()
    } else {
        patterns
            .iter()
            .map(|pattern| format!("'{pattern}'"))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// Resolve every member of an archive to the path it is written to, in
/// the same order as `names`, after sanitising and the optional proxy
/// rename.
///
/// Two different members that resolve to one destination are refused
/// rather than raced: the proxy rename maps every member basenamed
/// `reshade64.dll` or `dxgi.dll` onto a single path when the recipe
/// names no payload, so an archive holding both would otherwise install
/// whichever the reader happened to enumerate last.
fn archive_member_targets(
    names: &[String],
    proxy: Option<&str>,
    payload: Option<&str>,
) -> Result<Vec<ArchiveMember>, String> {
    let mut members = Vec::with_capacity(names.len());
    // Destination (compared case-insensitively, as the Windows
    // filesystem compares it) to the members claiming it, and the
    // contested destination, to the members claiming it. A `BTreeMap`
    // plus a `BTreeSet` in it is what makes the refusal the same string
    // whichever order the reader produced the members in.
    let mut claims: BTreeMap<String, (String, BTreeSet<String>)> = BTreeMap::new();
    for name in names {
        let Some(safe_name) = sanitize_archive_member(name) else {
            return Err(format!("extract-zip: unsafe archive member '{name}'."));
        };
        let target = archive_member_target(&safe_name, proxy, payload);
        let claim = claims
            .entry(target.to_ascii_lowercase())
            .or_insert_with(|| (target.clone(), BTreeSet::new()));
        // The lowest-spelled destination wins the message, so two
        // spellings of one Windows path still read the same way twice.
        if target < claim.0 {
            claim.0 = target.clone();
        }
        claim.1.insert(safe_name.clone());
        members.push(ArchiveMember { safe_name, target });
    }

    if let Some((target, claimants)) = claims
        .into_values()
        .find(|(_, claimants)| claimants.len() > 1)
    {
        let names: Vec<&str> = claimants.iter().map(String::as_str).collect();
        let mut listed = names
            .iter()
            .map(|name| format!("'{name}'"))
            .collect::<Vec<_>>();
        if listed.len() > 2 {
            let extra = listed.len() - 2;
            listed.truncate(2);
            listed.push(format!("and {extra} more"));
        }
        return Err(format!(
            "extract-zip: two archive members both resolve to '{target}' ({}). Two members cannot \
             install to one path, and picking a winner would be a coin flip. Declare the `payload` \
             param with the member that should take the proxy name, and narrow the archive with \
             `include` / `exclude` if the rest of it is not wanted.",
            listed.join(" and ")
        ));
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

/// Write a text file from a template.
///
/// `format: json` makes the step render for a document that something
/// else will parse. A `path`-typed config value rendered into JSON
/// unescaped produces a file no JSON parser will read, and a consumer
/// that returns `None` on a parse failure turns that into a silent
/// no-op — the override was written and the override did nothing. So in
/// that mode the placeholder is inserted as escaped JSON string
/// content, the recipe supplies the punctuation, and the rendered result
/// is parsed before a single byte is written.
fn run_write_text_file(step: &StepSpec, context: &StepContext<'_>) -> Result<StepResult, String> {
    let path = path_param(step, context.config, "pathField", "path")?;
    let resolved = resolve_write_target(step, context.executable_directory, &path)?;
    let template = param(step, "template")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let format = param_string(step, "format").unwrap_or("text");
    let rendered = match format {
        "text" => render_template(template, context.config),
        "json" => render_json_template(template, context.config),
        other => {
            return Err(format!(
                "write-text-file: format '{other}' is not a format this step knows. Use 'text' \
                 (the default) or 'json'."
            ))
        }
    };
    if format == "json" {
        // Refused before the write, not after: a file that exists and
        // does not parse is worse than one that does not exist, because
        // nothing downstream can tell the difference from a working one.
        if let Err(error) = serde_json::from_str::<JsonValue>(&rendered) {
            return Err(format!(
                "write-text-file: the rendered template is not valid JSON ({error}), so '{path}' \
                 was not written. A `path`-typed value inserted into a JSON string has to be \
                 escaped for JSON — declare `format: json`, keep the surrounding quotes in the \
                 template, and make sure every `{{name}}` in it names a declared config field."
            ));
        }
    }
    if let Some(parent) = resolved.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("write-text-file: could not create parent dir: {error}"))?;
    }
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

fn kill_process_name(step: &StepSpec, config: &ResolvedConfig) -> Result<String, String> {
    let name = path_param(step, config, "processNameField", "processName")?;
    if name.trim().is_empty() || name.contains(['*', '?', '/', '\\']) || name.starts_with('-') {
        return Err(
            "kill-process: process name must be a non-empty image name without wildcards or paths."
                .to_owned(),
        );
    }
    Ok(name)
}

fn run_kill_process(step: &StepSpec, context: &StepContext<'_>) -> Result<StepResult, String> {
    let process_name = kill_process_name(step, context.config)?;
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

/// How a `{name}` substitution is inserted into a rendered template.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Substitution {
    /// The config value, as it stands. What every existing step means
    /// by a placeholder.
    Verbatim,
    /// The config value as the *content* of a JSON string — every
    /// backslash, quote and control character escaped, with no
    /// surrounding quotes, because the recipe supplied those.
    JsonStringContent,
}

/// Render a step template: `{name}` from config, then `%NAME%` from the
/// process environment.
///
/// The order is the documented one and matters in both directions. A
/// config value that is itself `%LOCALAPPDATA%\…` is expanded, and a
/// config value that names a path containing a `{` is not re-scanned,
/// because the expansion pass runs over the substituted text rather than
/// over the template.
fn render_template(template: &str, config: &ResolvedConfig) -> String {
    expand_env_vars(&substitute_placeholders(
        template,
        config,
        Substitution::Verbatim,
    ))
}

/// [`render_template`] for a `format: json` step: the same two passes,
/// with the placeholder inserted JSON-escaped.
fn render_json_template(template: &str, config: &ResolvedConfig) -> String {
    expand_env_vars(&substitute_placeholders(
        template,
        config,
        Substitution::JsonStringContent,
    ))
}

/// `{name}` substitution alone, with no environment pass.
///
/// A `{` whose run to the next `}` does not name a declared config
/// field is left as text and scanning resumes *inside* the braces,
/// rather than consuming the run and putting it back. The rendered
/// result is the same either way for a template that is nothing but
/// placeholders, and it is the only way a document with braces of its
/// own survives: a JSON object opens with a `{` whose run to the first
/// `}` is `"key": "{value"`, and a scanner that consumed it would eat
/// the placeholder the recipe meant to fill in.
fn substitute_placeholders(
    template: &str,
    config: &ResolvedConfig,
    substitution: Substitution,
) -> String {
    let characters: Vec<char> = template.chars().collect();
    let mut output = String::with_capacity(template.len());
    let mut index = 0usize;
    while index < characters.len() {
        if characters[index] != '{' {
            output.push(characters[index]);
            index += 1;
            continue;
        }
        let close = characters[index + 1..]
            .iter()
            .position(|character| *character == '}')
            .map(|offset| index + 1 + offset);
        let Some(close) = close else {
            // No closing brace anywhere after this one.
            output.push('{');
            index += 1;
            continue;
        };
        let name: String = characters[index + 1..close].iter().collect();
        match config.get(&name).and_then(config_scalar) {
            Some(value) => {
                match substitution {
                    Substitution::Verbatim => output.push_str(&value),
                    Substitution::JsonStringContent => {
                        output.push_str(&json_string_content(&value))
                    }
                }
                index = close + 1;
            }
            None => {
                output.push('{');
                index += 1;
            }
        }
    }
    output
}

/// The body of a JSON string, escaped, without the quotes around it.
///
/// Exactly what `serde_json` writes inside a string: a backslash, a quote
/// and the two-character escapes for `\n`, `\r`, `\t`, `\b` and `\f` are
/// spelled out, and any other control character becomes `\u00xx`. A
/// Windows path is nothing but backslashes, so this is the difference
/// between a preference document `read_game_preference` can parse and
/// one it silently returns `None` for.
fn json_string_content(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            '\u{8}' => output.push_str("\\b"),
            '\u{c}' => output.push_str("\\f"),
            control if (control as u32) < 0x20 => {
                output.push_str(&format!("\\u{:04x}", control as u32));
            }
            other => output.push(other),
        }
    }
    output
}

/// Expand `%NAME%` from the process environment.
///
/// `NAME` is `[A-Za-z_][A-Za-z0-9_]*`, the shape Windows itself accepts.
/// A variable the process does not have is left **literal**: a template
/// that is prose, or a file's content, can contain a `%` for reasons that
/// have nothing to do with the environment, and quietly deleting it
/// would corrupt a value the recipe never meant to expand. There is no
/// `${VAR}` form — this is a Windows app, and `%LOCALAPPDATA%` is the
/// case that has to work.
fn expand_env_vars(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut output = String::with_capacity(text.len());
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            // Copy one whole character, so a multi-byte one is not split.
            let rest = &text[index..];
            let character = rest.chars().next().unwrap_or('\u{fffd}');
            output.push(character);
            index += character.len_utf8();
            continue;
        }
        match env_var_span(&text[index..]) {
            Some((name, consumed)) => {
                match std::env::var_os(name) {
                    Some(value) => output.push_str(&value.to_string_lossy()),
                    None => output.push_str(&text[index..index + consumed]),
                }
                index += consumed;
            }
            None => {
                output.push('%');
                index += 1;
            }
        }
    }
    output
}

/// The variable name in a leading `%NAME%`, and how many bytes it spans.
/// `None` when the text at this `%` is not a complete `%NAME%`.
fn env_var_span(text: &str) -> Option<(&str, usize)> {
    let rest = text.strip_prefix('%')?;
    let end = rest.find('%')?;
    let name = &rest[..end];
    let mut characters = name.chars();
    let valid = characters
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '_');
    valid.then_some((name, end + 2))
}

/// Resolve a path a step reads or writes, expanding `%NAME%` first so a
/// value that becomes absolute by expansion is treated as absolute.
///
/// The expanded path then goes through the caller's own check —
/// [`resolve_write_target`] for anything a step writes — so reaching a
/// location outside the install root needs the same `path`-typed config
/// field it always did, and no new exemption is granted for having come
/// from the environment.
fn resolve_path(base: &Path, candidate: &str) -> PathBuf {
    let expanded = expand_env_vars(candidate);
    let path = Path::new(&expanded);
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
///
/// This is also where a `%NAME%`-expanded path lands, because
/// `path_param` renders through [`render_template`], which expands the
/// environment. It gets no exemption for that: a value that expands to
/// an absolute path is an absolute path here, and one that expands to a
/// `..` walk out of the base is refused by the same walk below.
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
            // The same filter, the same payload lookup and the same
            // collision refusal the executor applies, so the rollback
            // set is the set of files the step is about to overwrite.
            let selection = select_archive_members(step, &names)?;
            let payload = declared_payload_member(step, context, &selection.names)?;
            for member in
                archive_member_targets(&selection.names, proxy.as_deref(), payload.as_deref())?
            {
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

    /// The REFramework shape, which is the reason the filter exists.
    /// v1.5.9.1 ships the injector, the OpenVR payload, the OpenXR
    /// loader and the autorun scripts in one archive, and the upstream
    /// release note tells a player with no headset to extract
    /// `dinput8.dll` and nothing else — "extracting the other files may
    /// crash a non-VR game".
    #[test]
    fn extract_zip_installs_only_the_members_include_selects() {
        let tree = TempTree::new("extract-include");
        let root = tree.0.clone();
        let archive = root.join("RE8.zip");
        write_zip(
            &archive,
            &[
                ("dinput8.dll", b"injector"),
                ("x64/dinput8.dll", b"the 64-bit injector"),
                ("openvr_api.dll", b"openvr"),
                ("openxr_loader.dll", b"openxr"),
                ("reframework/autorun/scripts/demo.lua", b"-- demo"),
            ],
        );

        let spec = step_of(
            "extract-zip",
            &[
                ("archivePath", json!(archive.to_string_lossy())),
                ("include", json!(["dinput8.dll"])),
            ],
        );
        let config = ResolvedConfig::default();

        let result = execute_step(&spec, &download_context(&config, &root))
            .expect("the filtered archive installs");
        // A pattern with no `/` in it is a basename, so it matches at
        // any depth: both copies of the injector land.
        assert_eq!(
            fs::read(root.join("dinput8.dll")).expect("injector"),
            b"injector"
        );
        assert_eq!(
            fs::read(root.join("x64").join("dinput8.dll")).expect("x64 injector"),
            b"the 64-bit injector"
        );
        for dropped in ["openvr_api.dll", "openxr_loader.dll", "reframework"] {
            assert!(
                !root.join(dropped).exists(),
                "'{dropped}' is not selected by include and must not be written"
            );
        }
        assert_eq!(
            result.affected_paths,
            vec!["dinput8.dll".to_owned(), "x64/dinput8.dll".to_owned()],
            "the transaction is handed the filtered set, not the archive"
        );
    }

    #[test]
    fn extract_zip_exclude_removes_from_the_included_set() {
        let tree = TempTree::new("extract-exclude");
        let root = tree.0.clone();
        let archive = root.join("RE8.zip");
        write_zip(
            &archive,
            &[
                ("dinput8.dll", b"injector"),
                ("reframework/reframework.ini", b"[General]\n"),
                ("reframework/autorun/scripts/new.lua", b"-- new"),
                ("reframework/autorun/legacy/old.lua", b"-- old"),
                ("reframework/autorun/legacy/deep/older.lua", b"-- older"),
                ("openvr_api.dll", b"openvr"),
            ],
        );

        let spec = step_of(
            "extract-zip",
            &[
                ("archivePath", json!(archive.to_string_lossy())),
                ("include", json!(["dinput8.dll", "reframework/**"])),
                ("exclude", json!(["reframework/autorun/legacy/*"])),
            ],
        );
        let config = ResolvedConfig::default();
        execute_step(&spec, &download_context(&config, &root)).expect("the archive installs");

        assert!(root.join("reframework").join("reframework.ini").is_file());
        assert!(root
            .join("reframework")
            .join("autorun")
            .join("scripts")
            .join("new.lua")
            .is_file());
        assert!(
            !root
                .join("reframework")
                .join("autorun")
                .join("legacy")
                .join("old.lua")
                .exists(),
            "exclude removed a member the include had selected"
        );
        // `*` does not cross a `/`, so the same pattern says nothing
        // about a member one directory deeper.
        assert!(
            root.join("reframework")
                .join("autorun")
                .join("legacy")
                .join("deep")
                .join("older.lua")
                .is_file(),
            "exclude was a single-segment glob, and * must not match past one /"
        );
        assert!(!root.join("openvr_api.dll").exists());
    }

    #[test]
    fn extract_zip_filters_the_members_of_a_seven_zip_too() {
        let tree = TempTree::new("extract-7z-include");
        let root = tree.0.clone();
        let archive = root.join("optiscaler.7z");
        write_7z(
            &archive,
            &[
                ("OptiScaler.dll", b"the payload"),
                ("OptiScaler.ini", b"[Upscaler]\n"),
                ("docs/readme.md", b"# readme"),
            ],
        );

        let spec = step_of(
            "extract-zip",
            &[
                ("archivePath", json!(archive.to_string_lossy())),
                // Lower-cased on purpose: the filter matches the way the
                // Windows filesystem it lands on does.
                ("include", json!(["optiscaler.dll"])),
            ],
        );
        let config = ResolvedConfig::default();

        let result = execute_step(&spec, &download_context(&config, &root))
            .expect("the filtered 7z installs");
        // The 7z reader hands its entries over in its own order, so the
        // filter is keyed on the member's name and not on a position in
        // a walk.
        assert_eq!(
            fs::read(root.join("OptiScaler.dll")).expect("payload"),
            b"the payload"
        );
        assert!(!root.join("OptiScaler.ini").exists());
        assert!(!root.join("docs").exists());
        assert_eq!(result.affected_paths, vec!["OptiScaler.dll".to_owned()]);
    }

    #[test]
    fn extract_zip_refuses_a_filter_that_selects_no_member() {
        let tree = TempTree::new("extract-empty-filter");
        let root = tree.0.clone();
        let archive = root.join("RE8.zip");
        write_zip(
            &archive,
            &[
                ("dinput8.dll", b"injector"),
                ("openvr_api.dll", b"openvr"),
                ("openxr_loader.dll", b"openxr"),
            ],
        );

        let spec = step_of(
            "extract-zip",
            &[
                ("archivePath", json!(archive.to_string_lossy())),
                ("include", json!(["reshade64.dll"])),
                ("exclude", json!(["*.lua"])),
            ],
        );
        let config = ResolvedConfig::default();

        let error = execute_step(&spec, &download_context(&config, &root))
            .expect_err("a filter that matches nothing is not an empty install");
        assert!(
            error.contains("selected none of the archive's 3 members"),
            "the error counts what the archive actually holds: {error}"
        );
        assert!(
            error.contains("'reshade64.dll'"),
            "the error names the pattern that matched nothing: {error}"
        );
        for written in ["dinput8.dll", "openvr_api.dll", "openxr_loader.dll"] {
            assert!(
                !root.join(written).exists(),
                "a refused install writes nothing: {error}"
            );
        }
    }

    #[test]
    fn extract_zip_refuses_a_filter_that_is_not_a_list_of_patterns() {
        let tree = TempTree::new("extract-bad-filter");
        let root = tree.0.clone();
        let archive = root.join("payload.zip");
        write_zip(&archive, &[("dinput8.dll", b"injector")]);
        let config = ResolvedConfig::default();

        for (params, expected) in [
            (
                vec![("include", json!("dinput8.dll"))],
                "not an array of glob patterns",
            ),
            (vec![("exclude", json!([7]))], "not a string"),
            (vec![("exclude", json!(["  "]))], "is empty"),
        ] {
            let mut all = vec![("archivePath", json!(archive.to_string_lossy()))];
            all.extend(params);
            let error = execute_step(
                &step_of("extract-zip", &all),
                &download_context(&config, &root),
            )
            .expect_err("a filter that is not a list of patterns is refused");
            assert!(error.contains(expected), "{error}");
        }
        assert!(
            !root.join("dinput8.dll").exists(),
            "a refused filter leaves the game folder alone"
        );
    }

    #[test]
    fn the_transaction_plans_the_filtered_members_not_the_whole_archive() {
        let tree = TempTree::new("extract-include-plan");
        let root = tree.0.clone();
        let archive = root.join("RE8.zip");
        write_zip(
            &archive,
            &[
                ("dinput8.dll", b"injector"),
                ("openvr_api.dll", b"openvr"),
                ("openxr_loader.dll", b"openxr"),
                ("reframework/autorun/scripts/demo.lua", b"-- demo"),
            ],
        );
        let spec = step_of(
            "extract-zip",
            &[
                ("archivePath", json!(archive.to_string_lossy())),
                ("include", json!(["dinput8.dll", "reframework/**"])),
            ],
        );
        let config = ResolvedConfig::default();

        let mut planned =
            plan_step_targets(&spec, &download_context(&config, &root)).expect("planned targets");
        planned.sort();
        let mut expected = vec![
            root.join("dinput8.dll"),
            root.join("reframework")
                .join("autorun")
                .join("scripts")
                .join("demo.lua"),
        ];
        expected.sort();
        assert_eq!(
            planned, expected,
            "preflight has to plan the same members the step writes, or the \
             rollback set is the archive rather than the install"
        );

        let result = execute_step(&spec, &download_context(&config, &root)).expect("it installs");
        let written: Vec<PathBuf> = result
            .affected_paths
            .iter()
            .map(|relative| root.join(relative))
            .collect();
        assert_eq!(
            written, expected,
            "run and preflight agree member for member"
        );
    }

    /// Two members claiming one destination is an authoring error, and
    /// the step refuses it instead of letting archive order pick.
    #[test]
    fn two_members_claiming_the_default_proxy_name_are_refused() {
        let tree = TempTree::new("extract-proxy-clash");
        let root = tree.0.clone();
        let archive = root.join("reshade.zip");
        write_zip(
            &archive,
            &[
                ("ReShade/ReShade64.dll", b"the injector"),
                ("ReShade/dxgi.dll", b"the dxgi proxy"),
                ("ReShade/ReShade.ini", b"[General]\n"),
            ],
        );

        let mut config = ResolvedConfig::default();
        config
            .values
            .insert("proxy".to_owned(), json!("winhttp.dll"));
        // No payload declared, so the ReShade-era default renames every
        // member basenamed reshade64.dll or dxgi.dll onto one path.
        let spec = step_of(
            "extract-zip",
            &[
                ("archivePath", json!(archive.to_string_lossy())),
                ("proxyField", json!("proxy")),
            ],
        );

        let error = execute_step(&spec, &download_context(&config, &root))
            .expect_err("two members cannot install to one path");
        assert!(error.contains("winhttp.dll"), "{error}");
        assert!(error.contains("ReShade/ReShade64.dll"), "{error}");
        assert!(error.contains("ReShade/dxgi.dll"), "{error}");
        assert!(
            error.contains("`payload`"),
            "the error says what the fix is: {error}"
        );
        assert!(
            !root.join("winhttp.dll").exists(),
            "a refused collision picks no winner: {error}"
        );
        assert!(
            !root.join("ReShade").join("ReShade64.dll").exists(),
            "a refused collision writes nothing at all: {error}"
        );
    }

    /// The same collision, with the members in the opposite order, has
    /// to produce the same refusal — otherwise "which one won" depends
    /// on how a reader happened to enumerate the archive.
    #[test]
    fn the_collision_refusal_does_not_depend_on_the_member_order() {
        let tree = TempTree::new("extract-proxy-clash-order");
        let root = tree.0.clone();
        let mut config = ResolvedConfig::default();
        config
            .values
            .insert("proxy".to_owned(), json!("winhttp.dll"));

        let first = root.join("first.zip");
        write_zip(
            &first,
            &[
                ("ReShade/ReShade64.dll", b"the injector"),
                ("ReShade/DXGI.dll", b"the dxgi proxy"),
            ],
        );
        let second = root.join("second.zip");
        write_zip(
            &second,
            &[
                ("ReShade/DXGI.dll", b"the dxgi proxy"),
                ("ReShade/ReShade64.dll", b"the injector"),
            ],
        );

        let refusal = |archive: &Path| -> String {
            let spec = step_of(
                "extract-zip",
                &[
                    ("archivePath", json!(archive.to_string_lossy())),
                    ("proxyField", json!("proxy")),
                ],
            );
            execute_step(&spec, &download_context(&config, &root))
                .expect_err("both archives carry the same authoring error")
        };
        assert_eq!(
            refusal(&first),
            refusal(&second),
            "the refusal is deterministic: a coin flip needs a stable \
             comparison to stop being a coin flip"
        );
    }

    /// A member that is already named like the proxy is a collision too:
    /// the rename would land on top of it.
    #[test]
    fn the_default_proxy_rename_refuses_to_land_on_a_member_of_the_same_name() {
        let tree = TempTree::new("extract-proxy-own-name");
        let root = tree.0.clone();
        let archive = root.join("payload.zip");
        write_zip(
            &archive,
            &[
                ("ReShade/ReShade64.dll", b"the injector"),
                ("winhttp.dll", b"a real winhttp"),
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
        let error = execute_step(&spec, &download_context(&config, &root))
            .expect_err("renaming onto an existing member of the archive is refused");
        assert!(error.contains("'ReShade/ReShade64.dll'"), "{error}");
        assert!(error.contains("'winhttp.dll'"), "{error}");
    }

    /// Declaring the payload is the fix the refusal names, and it has to
    /// work: the default rule is off once a payload is declared, so a
    /// second `reshade64.dll` keeps its own name and claims nothing.
    #[test]
    fn a_declared_payload_leaves_the_other_proxy_named_members_alone() {
        let tree = TempTree::new("extract-payload-immunity");
        let root = tree.0.clone();
        let archive = root.join("payload.zip");
        write_zip(
            &archive,
            &[
                ("ReShade/ReShade64.dll", b"the payload"),
                ("ReShade/samples/reshade64.dll", b"a bundled copy"),
                ("ReShade/dxgi.dll", b"the dxgi the archive ships"),
            ],
        );
        let mut config = ResolvedConfig::default();
        config
            .values
            .insert("proxy".to_owned(), json!("winhttp.dll"));
        config
            .values
            .insert("payloadDll".to_owned(), json!("ReShade64.dll"));
        let spec = step_of(
            "extract-zip",
            &[
                ("archivePath", json!(archive.to_string_lossy())),
                ("proxyField", json!("proxy")),
                ("payloadField", json!("payloadDll")),
            ],
        );

        let result = execute_step(&spec, &download_context(&config, &root))
            .expect("a declared payload is immune to the collision");
        assert_eq!(
            fs::read(root.join("winhttp.dll")).expect("proxy"),
            b"the payload"
        );
        assert_eq!(
            fs::read(root.join("ReShade").join("dxgi.dll")).expect("shipped dxgi"),
            b"the dxgi the archive ships"
        );
        assert_eq!(
            fs::read(root.join("ReShade").join("samples").join("reshade64.dll"))
                .expect("the bundled copy keeps its own name"),
            b"a bundled copy"
        );
        assert_eq!(result.affected_paths.len(), 3);
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

    /// The glob rules the `include` / `exclude` filter promises, stated
    /// over the matcher itself rather than over one archive.
    #[test]
    fn member_patterns_follow_the_documented_glob_rules() {
        let matches = |pattern: &str, name: &str| member_pattern_matches(pattern, name);

        // A pattern with no `/` is a basename, and matches at any depth.
        assert!(matches("dinput8.dll", "dinput8.dll"));
        assert!(matches("dinput8.dll", "x64/dinput8.dll"));
        assert!(matches(
            "dinput8.dll",
            "reframework/autorun/deep/dinput8.dll"
        ));
        assert!(!matches("dinput8.dll", "dinput8.dll.bak"));
        assert!(
            !matches("dinput8.dll", "x64/dinput8.dll.bak"),
            "a basename pattern is the whole basename, not a prefix"
        );

        // `*` stays inside one segment, `**` crosses them.
        assert!(matches(
            "reframework/autorun/*.lua",
            "reframework/autorun/a.lua"
        ));
        assert!(!matches(
            "reframework/autorun/*.lua",
            "reframework/autorun/deep/a.lua"
        ));
        assert!(matches("reframework/**/*.lua", "reframework/a.lua"));
        assert!(matches(
            "reframework/**/*.lua",
            "reframework/autorun/scripts/a.lua"
        ));
        assert!(matches("**", "a/b/c.dll"));
        assert!(matches("a/**/c", "a/b/c"));
        assert!(matches("a/**/c", "a/c"));
        assert!(!matches("a/**/c", "a/b/d"));
        assert!(matches("*.dll", "x64/OptiScaler.dll"));
        assert!(matches("optiscaler*.dll", "OptiScaler.dll"));
        assert!(!matches("optiscaler", "OptiScaler.dll"));

        // Windows comparisons are case-insensitive, and a member name
        // that differs only in case is the same file to the filesystem
        // it lands on.
        assert!(matches("DINPUT8.DLL", "dinput8.dll"));
        assert!(matches(
            "reframework/AUTORUN/**",
            "reframework/autorun/x.lua"
        ));

        // Nothing else is a wildcard. A Windows member name cannot hold
        // a `?`, so treating it as a literal cannot exclude a member the
        // recipe meant to install.
        assert!(!matches("dinput?.dll", "dinput8.dll"));
        assert!(matches("dinput?.dll", "dinput?.dll"));
    }

    /// One environment variable, set for the duration of a test and
    /// taken back out with it. The name carries a uuid because the suite
    /// runs its tests in parallel threads inside one process, and a
    /// shared `MODDIN_TEST_*` name would let one test read another's
    /// value.
    struct EnvVar(String);

    impl EnvVar {
        fn set(label: &str, value: &str) -> Self {
            let name = format!("MODDIN_TEST_{label}_{}", uuid::Uuid::new_v4().simple());
            std::env::set_var(&name, value);
            Self(name)
        }

        fn name(&self) -> &str {
            &self.0
        }
    }

    impl Drop for EnvVar {
        fn drop(&mut self) {
            std::env::remove_var(&self.0);
        }
    }

    #[test]
    fn a_template_expands_a_windows_environment_variable() {
        let local = EnvVar::set("LOCAL", "C:\\Users\\tester\\AppData\\Local");
        let config = ResolvedConfig::default();
        // The case the roadmap names: a `path`-typed config field has to
        // be able to say `%LOCALAPPDATA%` without hard-coding a profile.
        assert_eq!(
            render_template(
                &format!("%{}%\\Moddin\\profiles\\openxr", local.name()),
                &config
            ),
            "C:\\Users\\tester\\AppData\\Local\\Moddin\\profiles\\openxr"
        );

        // Expansion runs after the `{name}` substitution, so a config
        // value that is itself a `%VAR%` reference is expanded too.
        let profiles = EnvVar::set("PROFILES_ROOT", "D:\\profiles");
        let mut config = ResolvedConfig::default();
        config.values.insert(
            "profilesRoot".to_owned(),
            json!(format!("%{}%", profiles.name())),
        );
        assert_eq!(
            render_template("{profilesRoot}\\prefs.json", &config),
            "D:\\profiles\\prefs.json"
        );
    }

    #[test]
    fn an_unknown_or_stray_percent_stays_literal() {
        let config = ResolvedConfig::default();
        // Prose, a percentage, a shell fragment and file content all
        // carry `%` and `{}` for their own reasons. A variable the
        // process does not have is neither an error nor something to
        // delete.
        assert_eq!(
            render_template("100% of the time, %NOT_A_REAL_VARIABLE%, %%", &config),
            "100% of the time, %NOT_A_REAL_VARIABLE%, %%"
        );
        assert_eq!(
            render_template("${HOME} is bash, not this", &config),
            "${HOME} is bash, not this"
        );
        assert_eq!(render_template("a % b % c", &config), "a % b % c");
        assert_eq!(
            render_template("%1 and %PATH:1%", &config),
            "%1 and %PATH:1%",
            "a name Windows itself would not accept is left alone"
        );
    }

    #[test]
    fn a_path_that_becomes_absolute_by_expansion_is_absolute() {
        let outside = EnvVar::set("OUTSIDE", &std::env::temp_dir().to_string_lossy());
        let base = Path::new("C:\\games\\Some Game");
        assert_eq!(
            resolve_path(base, &format!("%{}%\\Moddin\\prefs.json", outside.name()),),
            std::env::temp_dir().join("Moddin").join("prefs.json"),
            "a path that is only absolute after expansion is treated as absolute"
        );
        // A relative candidate keeps its ordinary meaning.
        assert_eq!(
            resolve_path(base, "BepInEx\\core\\winhttp.dll"),
            base.join("BepInEx").join("core").join("winhttp.dll")
        );
    }

    #[test]
    fn a_step_writes_to_a_location_named_by_an_environment_variable() {
        let tree = TempTree::new("env-write-path");
        let root = tree.0.clone();
        let profiles = root.join("profiles");
        fs::create_dir_all(&profiles).expect("profiles directory");
        let env = EnvVar::set("PROFILES", profiles.to_string_lossy().as_ref());
        let config = ResolvedConfig::default();

        let spec = step_of(
            "write-text-file",
            &[
                (
                    "path",
                    json!(format!("%{}%\\openxr\\cyberpunk.json", env.name())),
                ),
                ("template", json!("runtime=steamvr")),
            ],
        );
        let result = execute_step(&spec, &download_context(&config, &root))
            .expect("the step reaches the directory the variable names");
        let written = profiles.join("openxr").join("cyberpunk.json");
        assert!(written.is_file(), "{}", written.display());
        assert_eq!(
            result.affected_paths,
            vec![written.to_string_lossy().into_owned()]
        );
    }

    /// Reaching outside the install root needs the same `path`-typed
    /// config field it always did. Expansion is not a new exemption, so
    /// a variable whose value walks out of the root is refused by the
    /// same check that refuses `..` in a literal.
    #[test]
    fn an_expanded_path_that_walks_out_of_the_root_is_still_refused() {
        let tree = TempTree::new("env-escape");
        let root = tree.0.clone();
        let escape = EnvVar::set("ESCAPE", "..");
        let config = ResolvedConfig::default();

        let spec = step_of(
            "write-text-file",
            &[
                ("path", json!(format!("%{}%\\hijacked.ini", escape.name()))),
                ("template", json!("[General]")),
            ],
        );
        let error = execute_step(&spec, &download_context(&config, &root))
            .expect_err("expansion does not exempt a path from the install-root rule");
        assert!(error.contains("escapes the install directory"), "{error}");
        assert!(
            !root
                .parent()
                .expect("a parent")
                .join("hijacked.ini")
                .exists(),
            "nothing was written outside the root"
        );
    }

    #[test]
    fn write_text_file_in_json_format_escapes_the_path_and_parses() {
        let tree = TempTree::new("json-write");
        let root = tree.0.clone();
        // The OpenXR preference document: a JSON file under
        // %LOCALAPPDATA% whose one value is a Windows path. Rendered
        // unescaped it is not JSON, and `read_game_preference` returns
        // None on a file it cannot parse — a silent no-op override.
        let mut config = ResolvedConfig::default();
        config.values.insert(
            "preferredRuntime".to_owned(),
            json!("C:\\Program Files\\Moddin\\openxr_runtime.json"),
        );
        let spec = step_of(
            "write-text-file",
            &[
                ("path", json!("profiles\\openxr\\cyberpunk.json")),
                ("format", json!("json")),
                (
                    "template",
                    json!("{\"runtimePath\": \"{preferredRuntime}\", \"enabled\": true}"),
                ),
            ],
        );

        execute_step(&spec, &download_context(&config, &root))
            .expect("the JSON document is written");
        let written =
            fs::read_to_string(root.join("profiles").join("openxr").join("cyberpunk.json"))
                .expect("the preference document");
        assert!(
            written.contains(r"C:\\Program Files"),
            "the backslashes are escaped: {written}"
        );
        let parsed: JsonValue = serde_json::from_str(&written)
            .expect("the document parses, so nothing downstream returns None");
        assert_eq!(
            parsed["runtimePath"].as_str(),
            Some("C:\\Program Files\\Moddin\\openxr_runtime.json"),
            "the value round-trips byte for byte"
        );
        assert_eq!(parsed["enabled"].as_bool(), Some(true));
    }

    #[test]
    fn write_text_file_in_json_format_refuses_what_it_cannot_parse() {
        let tree = TempTree::new("json-refuse");
        let root = tree.0.clone();
        let mut config = ResolvedConfig::default();
        config
            .values
            .insert("preferredRuntime".to_owned(), json!("C:\\xr\\steamvr.json"));
        // A recipe that drops the closing brace, which is exactly what
        // an unescaped Windows path does to a hand-written document.
        // Nothing is written: a file that exists and does not parse is
        // indistinguishable from a working one to whatever reads it.
        let spec = step_of(
            "write-text-file",
            &[
                ("path", json!("profiles\\openxr\\cyberpunk.json")),
                ("format", json!("json")),
                (
                    "template",
                    json!("{\"runtimePath\": \"{preferredRuntime}\""),
                ),
            ],
        );
        let error = execute_step(&spec, &download_context(&config, &root))
            .expect_err("a document that would not parse is not written");
        assert!(error.contains("not valid JSON"), "{error}");
        assert!(
            !root.join("profiles").exists(),
            "nothing is created for a refused document: {error}"
        );

        // The same document without `format: json` is what the roadmap
        // complained about: written, and silently unreadable.
        let unescaped = step_of(
            "write-text-file",
            &[
                ("path", json!("profiles\\openxr\\cyberpunk.json")),
                (
                    "template",
                    json!("{\"runtimePath\": \"{preferredRuntime}\""),
                ),
            ],
        );
        execute_step(&unescaped, &download_context(&config, &root)).expect("plain text writes");
        let written =
            fs::read_to_string(root.join("profiles").join("openxr").join("cyberpunk.json"))
                .expect("the plain-text document");
        assert!(
            serde_json::from_str::<JsonValue>(&written).is_err(),
            "this is the failure the format exists to catch: {written}"
        );
    }

    #[test]
    fn write_text_file_keeps_plain_text_as_the_default() {
        let tree = TempTree::new("text-default");
        let root = tree.0.clone();
        let mut config = ResolvedConfig::default();
        config
            .values
            .insert("note".to_owned(), json!("a \"quoted\" value"));

        // No `format`, so the placeholder is the value verbatim — which
        // is what every step that shipped before `format: json` means,
        // and what a file of prose or INI wants.
        let spec = step_of(
            "write-text-file",
            &[
                ("path", json!("tray.ini")),
                ("template", json!("note={note}")),
            ],
        );
        execute_step(&spec, &download_context(&config, &root)).expect("plain text still writes");
        assert_eq!(
            fs::read_to_string(root.join("tray.ini")).expect("ini"),
            "note=a \"quoted\" value"
        );

        // An unknown format is an authoring error, not a silent fallback
        // to plain text: a recipe that asked for something the step
        // cannot do has to be told so.
        let spec = step_of(
            "write-text-file",
            &[
                ("path", json!("tray.ini")),
                ("format", json!("yaml")),
                ("template", json!("note: {note}")),
            ],
        );
        let error = execute_step(&spec, &download_context(&config, &root))
            .expect_err("a format the step does not know is refused");
        assert!(error.contains("not a format this step knows"), "{error}");
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
    fn kill_process_resolves_a_config_value_and_rejects_missing_or_broad_names() {
        let mut config = ResolvedConfig::default();
        config.values.insert("image".to_owned(), json!("game.exe"));
        let step = step_of("kill-process", &[("processNameField", json!("image"))]);
        assert_eq!(
            kill_process_name(&step, &config).expect("field resolves"),
            "game.exe"
        );
        assert!(kill_process_name(&step, &ResolvedConfig::default()).is_err());
        for name in ["", "*.exe", "game?.exe", "C:\\game.exe"] {
            let step = step_of("kill-process", &[("processName", json!(name))]);
            assert!(kill_process_name(&step, &config).is_err(), "{name}");
        }
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
    /// The run between the braces has to look like a field name — the
    /// same `[A-Za-z_][A-Za-z0-9_]*` the community validator accepts.
    /// A `format: json` template is a document with braces of its own
    /// (`{"runtimePath": "{preferredRuntime}"}`), and a scanner that
    /// took everything up to the next `}` would report the first two as
    /// one undeclared field, failing every recipe the JSON format
    /// exists to enable.
    ///
    /// Shared with the recipe-wide invariant in `capability_runner`,
    /// which is about specs rather than about this module's steps.
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
                    for (index, character) in text.char_indices() {
                        if character != '{' {
                            continue;
                        }
                        let rest = &text[index + 1..];
                        let name: String = rest
                            .chars()
                            .take_while(|character| {
                                character.is_ascii_alphanumeric() || *character == '_'
                            })
                            .collect();
                        let is_field_name = name
                            .chars()
                            .next()
                            .is_some_and(|first| first.is_ascii_alphabetic() || first == '_');
                        if is_field_name && rest[name.len()..].starts_with('}') {
                            names.insert(name);
                        }
                    }
                }
            }
        }
        names
    }

    /// A JSON template is a document with braces in it, so the scan that
    /// feeds the recipe-wide invariant has to see the placeholder and
    /// not the punctuation around it. The same holds for the `{` of a
    /// `{}` pair and for a `{` with no closing brace.
    #[test]
    fn a_json_template_reports_its_placeholders_and_not_its_punctuation() {
        let spec: crate::capability::CapabilitySpec = serde_yaml::from_str(
            r#"
id: json-writer
displayName: JSON writer
category: qol
status: available
configSchema:
  - name: preferredRuntime
    type: path
    required: true
install:
  - kind: write-text-file
    params:
      path: prefs.json
      format: json
      template: '{"runtimePath": "{preferredRuntime}", "note": "{}", "literal": "{undeclared}" }'
"#,
        )
        .expect("the synthetic spec parses");

        let names = config_fields_a_step_reads(&spec);
        assert!(
            names.contains("preferredRuntime"),
            "the placeholder the runner would fill in: {names:?}"
        );
        assert!(
            !names.contains("{}") && !names.iter().any(|name| name.contains('"')),
            "JSON punctuation must not be read as a config field: {names:?}"
        );
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
