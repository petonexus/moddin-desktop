use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    env, fs,
    io::Write,
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionFile {
    pub target_path: String,
    pub backup_path: Option<String>,
    pub existed_before: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionRecord {
    pub id: String,
    pub created_at: u64,
    pub kind: String,
    pub label: String,
    pub game_id: String,
    pub target_path: String,
    pub backup_path: String,
    pub status: String,
    #[serde(default)]
    pub files: Vec<TransactionFile>,
    #[serde(default)]
    pub created_directories: Vec<String>,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

fn now_millis() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .map_err(|error| format!("System clock error: {error}"))
}

fn transaction_root() -> PathBuf {
    env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir)
        .join("Moddin")
        .join("transactions")
}

fn is_valid_transaction_id(id: &str) -> bool {
    let Some((timestamp, nonce)) = id.split_once('-') else {
        return false;
    };

    !timestamp.is_empty()
        && timestamp.chars().all(|character| character.is_ascii_digit())
        && nonce.len() == 32
        && nonce.chars().all(|character| character.is_ascii_hexdigit())
}

fn transaction_dir(id: &str) -> Result<PathBuf, String> {
    if !is_valid_transaction_id(id) {
        return Err("Invalid Moddin transaction id.".to_owned());
    }
    Ok(transaction_root().join(id))
}

fn manifest_path(id: &str) -> Result<PathBuf, String> {
    Ok(transaction_dir(id)?.join("manifest.json"))
}

fn manifest_backup_path(id: &str) -> Result<PathBuf, String> {
    Ok(transaction_dir(id)?.join("manifest.json.bak"))
}

fn validate_record(record: &TransactionRecord, expected_id: &str) -> Result<(), String> {
    if record.id != expected_id || !is_valid_transaction_id(&record.id) {
        return Err(format!(
            "Transaction manifest identity mismatch for '{expected_id}'."
        ));
    }
    if !matches!(record.status.as_str(), "prepared" | "applied" | "rolled_back") {
        return Err(format!(
            "Transaction '{}' has an invalid status '{}'.",
            record.id, record.status
        ));
    }
    Ok(())
}

fn ensure_target_within_root(target_root: &Path, target: &Path) -> Result<(), String> {
    let relative = target.strip_prefix(target_root).map_err(|_| {
        format!(
            "Transaction target escapes its allowed root: {}",
            target.display()
        )
    })?;

    if relative.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(format!(
            "Transaction target contains an unsafe path traversal: {}",
            target.display()
        ));
    }

    Ok(())
}

fn write_record(record: &TransactionRecord) -> Result<(), String> {
    validate_record(record, &record.id)?;
    let directory = transaction_dir(&record.id)?;
    fs::create_dir_all(&directory)
        .map_err(|error| format!("Could not create transaction directory: {error}"))?;

    let json = serde_json::to_vec_pretty(record)
        .map_err(|error| format!("Could not serialize transaction: {error}"))?;
    let manifest = manifest_path(&record.id)?;
    let backup = manifest_backup_path(&record.id)?;
    let temporary = directory.join(format!(
        "manifest.json.tmp-{}",
        uuid::Uuid::new_v4().simple()
    ));

    let write_result = (|| -> Result<(), String> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| format!("Could not create temporary transaction manifest: {error}"))?;
        file.write_all(&json)
            .map_err(|error| format!("Could not write temporary transaction manifest: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("Could not flush temporary transaction manifest: {error}"))?;
        drop(file);

        if manifest.is_file() {
            fs::copy(&manifest, &backup)
                .map_err(|error| format!("Could not preserve previous transaction manifest: {error}"))?;
            fs::remove_file(&manifest)
                .map_err(|error| format!("Could not replace transaction manifest: {error}"))?;
        }

        fs::rename(&temporary, &manifest)
            .map_err(|error| format!("Could not commit transaction manifest: {error}"))?;
        Ok(())
    })();

    if write_result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    write_result
}

fn parse_record(contents: &str, id: &str) -> Result<TransactionRecord, String> {
    let record = serde_json::from_str::<TransactionRecord>(contents)
        .map_err(|error| format!("Could not parse transaction '{id}': {error}"))?;
    validate_record(&record, id)?;
    Ok(record)
}

fn read_record(id: &str) -> Result<TransactionRecord, String> {
    let manifest = manifest_path(id)?;
    let backup = manifest_backup_path(id)?;

    let primary_error = match fs::read_to_string(&manifest) {
        Ok(contents) => match parse_record(&contents, id) {
            Ok(record) => return Ok(record),
            Err(error) => error,
        },
        Err(error) => format!("Could not read transaction '{id}': {error}"),
    };

    if let Ok(contents) = fs::read_to_string(&backup) {
        if let Ok(record) = parse_record(&contents, id) {
            return Ok(record);
        }
    }

    Err(primary_error)
}

fn is_process_running(image_name: &str) -> bool {
    crate::process::is_process_running(image_name)
}

fn running_obs_executable() -> Option<PathBuf> {
    let output = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-Command",
            "(Get-Process obs64 -ErrorAction SilentlyContinue | Select-Object -First 1 -ExpandProperty Path)",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;

    let path = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if path.is_empty() {
        None
    } else {
        Some(PathBuf::from(path))
    }
}

fn close_obs_gracefully() -> Result<bool, String> {
    if !is_process_running("obs64.exe") {
        return Ok(false);
    }

    let status = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-Command",
            "$p = Get-Process obs64 -ErrorAction SilentlyContinue; if ($p) { $p | ForEach-Object { [void]$_.CloseMainWindow() } }",
        ])
        .status()
        .map_err(|error| format!("Could not ask OBS to close before rollback: {error}"))?;

    if !status.success() {
        return Err(
            "Windows could not request a graceful OBS shutdown before rollback.".to_owned(),
        );
    }

    for _ in 0..40 {
        if !is_process_running("obs64.exe") {
            return Ok(true);
        }
        thread::sleep(Duration::from_millis(500));
    }

    Err("OBS did not close within 20 seconds. Close it manually and try Undo again.".to_owned())
}

fn reopen_obs(path: Option<&Path>) {
    let Some(path) = path else {
        return;
    };
    if !path.is_file() {
        return;
    }

    let mut command = Command::new(path);
    if let Some(parent) = path.parent() {
        command.current_dir(parent);
    }
    let _ = command.spawn();
}

fn new_transaction_id() -> Result<(String, u64), String> {
    let created_at = now_millis()?;
    let id = format!("{}-{}", created_at, uuid::Uuid::new_v4().simple());
    Ok((id, created_at))
}

pub fn backup_file_with_metadata(
    target: &Path,
    kind: &str,
    label: &str,
    game_id: &str,
    metadata: BTreeMap<String, String>,
) -> Result<TransactionRecord, String> {
    if !target.is_file() {
        return Err(format!("Cannot back up missing file: {}", target.display()));
    }

    let (id, created_at) = new_transaction_id()?;
    let directory = transaction_dir(&id)?;
    fs::create_dir_all(&directory)
        .map_err(|error| format!("Could not create transaction directory: {error}"))?;

    let original_name = target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("original.bin");
    let backup_path = directory.join(format!("backup-{original_name}"));

    fs::copy(target, &backup_path)
        .map_err(|error| format!("Could not back up '{}': {error}", target.display()))?;

    let record = TransactionRecord {
        id,
        created_at,
        kind: kind.to_owned(),
        label: label.to_owned(),
        game_id: game_id.to_owned(),
        target_path: target.to_string_lossy().into_owned(),
        backup_path: backup_path.to_string_lossy().into_owned(),
        status: "applied".to_owned(),
        files: vec![TransactionFile {
            target_path: target.to_string_lossy().into_owned(),
            backup_path: Some(backup_path.to_string_lossy().into_owned()),
            existed_before: true,
        }],
        created_directories: Vec::new(),
        metadata,
    };

    write_record(&record)?;
    Ok(record)
}

pub fn begin_file_set_transaction(
    target_root: &Path,
    targets: &[PathBuf],
    kind: &str,
    label: &str,
    game_id: &str,
    metadata: BTreeMap<String, String>,
) -> Result<TransactionRecord, String> {
    let (id, created_at) = new_transaction_id()?;
    let directory = transaction_dir(&id)?;
    let backup_directory = directory.join("files");
    fs::create_dir_all(&backup_directory)
        .map_err(|error| format!("Could not create transaction backup directory: {error}"))?;

    let mut unique_targets = targets.to_vec();
    unique_targets.sort();
    unique_targets.dedup();

    let mut files = Vec::with_capacity(unique_targets.len());
    for (index, target) in unique_targets.iter().enumerate() {
        ensure_target_within_root(target_root, target)?;

        let existed_before = target.is_file();
        let backup_path = if existed_before {
            let file_name = target
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("file.bin");
            let backup = backup_directory.join(format!("{index:04}-{file_name}"));
            fs::copy(target, &backup).map_err(|error| {
                format!(
                    "Could not back up existing file '{}': {error}",
                    target.display()
                )
            })?;
            Some(backup.to_string_lossy().into_owned())
        } else {
            None
        };

        files.push(TransactionFile {
            target_path: target.to_string_lossy().into_owned(),
            backup_path,
            existed_before,
        });
    }

    let mut directories = Vec::new();
    for target in &unique_targets {
        let mut current = target.parent();
        while let Some(directory) = current {
            if directory == target_root || !directory.starts_with(target_root) {
                break;
            }
            if !directory.exists() {
                directories.push(directory.to_string_lossy().into_owned());
            }
            current = directory.parent();
        }
    }
    directories.sort_by_key(|path| std::cmp::Reverse(Path::new(path).components().count()));
    directories.dedup();

    let record = TransactionRecord {
        id,
        created_at,
        kind: kind.to_owned(),
        label: label.to_owned(),
        game_id: game_id.to_owned(),
        target_path: target_root.to_string_lossy().into_owned(),
        backup_path: backup_directory.to_string_lossy().into_owned(),
        status: "prepared".to_owned(),
        files,
        created_directories: directories,
        metadata,
    };

    write_record(&record)?;
    Ok(record)
}

pub fn mark_applied(mut record: TransactionRecord) -> Result<TransactionRecord, String> {
    record.status = "applied".to_owned();
    write_record(&record)?;
    Ok(record)
}

pub fn restore_record(mut record: TransactionRecord) -> Result<TransactionRecord, String> {
    validate_record(&record, &record.id)?;

    if record.files.is_empty() {
        let backup = PathBuf::from(&record.backup_path);
        let target = PathBuf::from(&record.target_path);
        if !backup.is_file() {
            return Err(format!("Backup no longer exists: {}", backup.display()));
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Could not recreate target directory: {error}"))?;
        }
        fs::copy(&backup, &target).map_err(|error| {
            format!(
                "Could not restore '{}' from '{}': {error}",
                target.display(),
                backup.display()
            )
        })?;
    } else {
        for file in record.files.iter().rev() {
            let target = PathBuf::from(&file.target_path);
            if file.existed_before {
                let backup = file
                    .backup_path
                    .as_ref()
                    .map(PathBuf::from)
                    .ok_or_else(|| format!("Missing backup metadata for {}", target.display()))?;
                if !backup.is_file() {
                    return Err(format!("Backup no longer exists: {}", backup.display()));
                }
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)
                        .map_err(|error| format!("Could not recreate target directory: {error}"))?;
                }
                fs::copy(&backup, &target).map_err(|error| {
                    format!("Could not restore '{}': {error}", target.display())
                })?;
            } else if target.exists() {
                fs::remove_file(&target).map_err(|error| {
                    format!(
                        "Could not remove created file '{}': {error}",
                        target.display()
                    )
                })?;
            }
        }

        for directory in &record.created_directories {
            let path = PathBuf::from(directory);
            if path.is_dir() {
                let _ = fs::remove_dir(&path);
            }
        }
    }

    record.status = "rolled_back".to_owned();
    write_record(&record)?;
    Ok(record)
}

pub fn list_transactions_sync() -> Result<Vec<TransactionRecord>, String> {
    let root = transaction_root();
    if !root.exists() {
        return Ok(Vec::new());
    }

    let mut records = Vec::new();
    for entry in fs::read_dir(&root)
        .map_err(|error| format!("Could not read transaction directory: {error}"))?
    {
        let entry = entry.map_err(|error| format!("Could not read transaction entry: {error}"))?;
        if !entry.path().is_dir() {
            continue;
        }

        let Some(id) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if let Ok(record) = read_record(&id) {
            records.push(record);
        }
    }

    records.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(records)
}

#[tauri::command]
pub async fn list_transactions() -> Result<Vec<TransactionRecord>, String> {
    tauri::async_runtime::spawn_blocking(list_transactions_sync)
        .await
        .map_err(|error| format!("Transaction history task failed: {error}"))?
}

#[tauri::command]
pub fn rollback_latest_module_transaction(
    game_id: String,
    kind: String,
) -> Result<TransactionRecord, String> {
    let record = list_transactions_sync()?
        .into_iter()
        .find(|record| {
            record.status == "applied" && record.game_id == game_id && record.kind == kind
        })
        .ok_or_else(|| {
            format!("No active transaction was found for module '{kind}' in game '{game_id}'.")
        })?;

    rollback_transaction(record.id)
}

#[tauri::command]
pub fn rollback_transaction(id: String) -> Result<TransactionRecord, String> {
    let record = read_record(&id)?;
    if record.status == "rolled_back" {
        return Ok(record);
    }

    if let Some(process_name) = record.metadata.get("processName") {
        if is_process_running(process_name) {
            return Err(format!(
                "Close {process_name} before undoing this transaction."
            ));
        }
    }

    if record.kind != "obs-vr" {
        return restore_record(record);
    }

    let obs_path = running_obs_executable().or_else(|| {
        let default = PathBuf::from(r"C:\Program Files\obs-studio\bin\64bit\obs64.exe");
        default.is_file().then_some(default)
    });
    let obs_was_open = close_obs_gracefully()?;

    let restored = restore_record(record);
    if obs_was_open {
        reopen_obs(obs_path.as_deref());
    }
    restored
}

// ---------------------------------------------------------------------------
// Named snapshots — point-in-time rollback points
// ---------------------------------------------------------------------------

/// A `Snapshot` records a name + the list of `applied` transaction ids
/// that existed at the moment the snapshot was created. Rolling a
/// snapshot back iterates the list in **reverse chronological order**
/// (newest first) so the restore order matches the natural undo order.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub id: String,
    pub name: String,
    pub created_at: u64,
    pub game_id: String,
    pub transaction_ids: Vec<String>,
}

fn snapshot_root() -> PathBuf {
    env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir)
        .join("Moddin")
        .join("snapshots")
}

fn is_valid_snapshot_id(id: &str) -> bool {
    is_valid_transaction_id(id)
}

fn snapshot_path(id: &str) -> Result<PathBuf, String> {
    if !is_valid_snapshot_id(id) {
        return Err("Invalid Moddin snapshot id.".to_owned());
    }
    Ok(snapshot_root().join(format!("{id}.json")))
}

fn write_snapshot(snapshot: &Snapshot) -> Result<(), String> {
    let path = snapshot_path(&snapshot.id)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Could not create snapshot directory: {error}"))?;
    }
    let json = serde_json::to_vec_pretty(snapshot)
        .map_err(|error| format!("Could not serialize snapshot: {error}"))?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, &json).map_err(|error| format!("Could not write snapshot: {error}"))?;
    fs::rename(&tmp, &path).map_err(|error| format!("Could not commit snapshot: {error}"))?;
    Ok(())
}

fn read_snapshot(id: &str) -> Result<Snapshot, String> {
    let path = snapshot_path(id)?;
    let contents = fs::read_to_string(&path)
        .map_err(|error| format!("Could not read snapshot '{id}': {error}"))?;
    let snapshot: Snapshot = serde_json::from_str(&contents)
        .map_err(|error| format!("Could not parse snapshot '{id}': {error}"))?;
    if snapshot.id != id {
        return Err(format!("Snapshot identity mismatch for '{id}'."));
    }
    Ok(snapshot)
}

/// Create a named snapshot of every currently `applied` transaction for
/// the given game. The snapshot itself is stored under
/// `%LOCALAPPDATA%/Moddin/snapshots/` so the regular transaction log
/// stays untouched and the snapshot can outlive a later `undo`.
#[tauri::command]
pub fn create_snapshot(name: String, game_id: String) -> Result<Snapshot, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("Snapshot name is required.".to_owned());
    }
    if game_id.is_empty() {
        return Err("Snapshot game_id is required.".to_owned());
    }
    let (id_part, created_at) = new_transaction_id()?;
    // Snapshot ids share the transaction-id shape (timestamp-nonce) so
    // existing path-validation rules apply unchanged.
    let snapshot = Snapshot {
        id: id_part,
        name: trimmed.to_owned(),
        created_at,
        game_id,
        transaction_ids: list_transactions_sync()?
            .into_iter()
            .filter(|record| record.status == "applied" && record.game_id == snapshot_game_filter(&record))
            .map(|record| record.id)
            .collect(),
    };
    let _ = snapshot_game_filter; // placeholder for future per-game filtering
    write_snapshot(&snapshot)?;
    Ok(snapshot)
}

fn snapshot_game_filter(record: &TransactionRecord) -> String {
    record.game_id.clone()
}

#[tauri::command]
pub fn list_snapshots() -> Result<Vec<Snapshot>, String> {
    let root = snapshot_root();
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut snapshots = Vec::new();
    for entry in fs::read_dir(&root)
        .map_err(|error| format!("Could not read snapshot directory: {error}"))?
    {
        let entry = entry.map_err(|error| format!("Could not read snapshot entry: {error}"))?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(id) = path.file_stem().and_then(|value| value.to_str()).map(str::to_owned) else {
            continue;
        };
        if let Ok(snapshot) = read_snapshot(&id) {
            snapshots.push(snapshot);
        }
    }
    snapshots.sort_by(|left, right| right.created_at.cmp(&left.created_at));
    Ok(snapshots)
}

/// Roll back every transaction captured by the snapshot, newest first.
/// Already rolled-back transactions are skipped (so a snapshot can be
/// re-applied to undo a partial restore).
#[tauri::command]
pub fn rollback_snapshot(id: String) -> Result<Vec<TransactionRecord>, String> {
    let snapshot = read_snapshot(&id)?;
    let mut restored = Vec::new();
    for transaction_id in snapshot.transaction_ids.iter().rev() {
        let record = read_record(transaction_id)?;
        if record.status == "rolled_back" {
            continue;
        }
        let rolled = rollback_transaction(transaction_id.clone())?;
        restored.push(rolled);
    }
    Ok(restored)
}

#[tauri::command]
pub fn delete_snapshot(id: String) -> Result<(), String> {
    let path = snapshot_path(&id)?;
    if path.is_file() {
        fs::remove_file(&path)
            .map_err(|error| format!("Could not delete snapshot: {error}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transaction_id_validation_accepts_generated_shape() {
        assert!(is_valid_transaction_id(
            "1757720000000-0123456789abcdef0123456789abcdef"
        ));
    }

    #[test]
    fn transaction_id_validation_rejects_path_traversal_and_malformed_ids() {
        for id in [
            "../manifest",
            "..-0123456789abcdef0123456789abcdef",
            "1757720000000/../../escape",
            "1757720000000-short",
            "not-a-timestamp-0123456789abcdef0123456789abcdef",
            "1757720000000-0123456789abcdef0123456789abcdeg",
        ] {
            assert!(!is_valid_transaction_id(id), "unexpectedly accepted {id}");
        }
    }

    #[test]
    fn target_validation_accepts_descendants() {
        let root = env::temp_dir().join("moddin-target-root");
        let target = root.join("subdir").join("plugin.dll");
        assert!(ensure_target_within_root(&root, &target).is_ok());
    }

    #[test]
    fn target_validation_rejects_lexical_parent_escape() {
        let root = env::temp_dir().join("moddin-target-root");
        let target = root
            .join("subdir")
            .join("..")
            .join("..")
            .join("outside.dll");
        assert!(ensure_target_within_root(&root, &target).is_err());
    }

    #[test]
    fn target_validation_rejects_sibling_path() {
        let root = env::temp_dir().join("moddin-target-root");
        let sibling = root
            .parent()
            .expect("temp child must have a parent")
            .join("moddin-sibling")
            .join("outside.dll");
        assert!(ensure_target_within_root(&root, &sibling).is_err());
    }

    #[test]
    fn record_validation_rejects_identity_and_status_corruption() {
        let id = "1757720000000-0123456789abcdef0123456789abcdef";
        let mut record = TransactionRecord {
            id: id.to_owned(),
            created_at: 1,
            kind: "test".to_owned(),
            label: "test".to_owned(),
            game_id: "game".to_owned(),
            target_path: "target".to_owned(),
            backup_path: "backup".to_owned(),
            status: "applied".to_owned(),
            files: Vec::new(),
            created_directories: Vec::new(),
            metadata: BTreeMap::new(),
        };

        assert!(validate_record(&record, id).is_ok());
        assert!(validate_record(&record, "1757720000001-0123456789abcdef0123456789abcdef")
            .is_err());

        record.status = "mystery".to_owned();
        assert!(validate_record(&record, id).is_err());
    }
}
