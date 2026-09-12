use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
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

fn transaction_dir(id: &str) -> PathBuf {
    transaction_root().join(id)
}

fn manifest_path(id: &str) -> PathBuf {
    transaction_dir(id).join("manifest.json")
}

fn write_record(record: &TransactionRecord) -> Result<(), String> {
    let directory = transaction_dir(&record.id);
    fs::create_dir_all(&directory)
        .map_err(|error| format!("Could not create transaction directory: {error}"))?;

    let json = serde_json::to_string_pretty(record)
        .map_err(|error| format!("Could not serialize transaction: {error}"))?;

    fs::write(manifest_path(&record.id), json)
        .map_err(|error| format!("Could not save transaction manifest: {error}"))
}

fn read_record(id: &str) -> Result<TransactionRecord, String> {
    let contents = fs::read_to_string(manifest_path(id))
        .map_err(|error| format!("Could not read transaction '{id}': {error}"))?;

    serde_json::from_str(&contents)
        .map_err(|error| format!("Could not parse transaction '{id}': {error}"))
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
    let directory = transaction_dir(&id);
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
    let directory = transaction_dir(&id);
    let backup_directory = directory.join("files");
    fs::create_dir_all(&backup_directory)
        .map_err(|error| format!("Could not create transaction backup directory: {error}"))?;

    let mut unique_targets = targets.to_vec();
    unique_targets.sort();
    unique_targets.dedup();

    let mut files = Vec::with_capacity(unique_targets.len());
    for (index, target) in unique_targets.iter().enumerate() {
        if !target.starts_with(target_root) {
            return Err(format!(
                "Transaction target escapes the selected game directory: {}",
                target.display()
            ));
        }

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

        let manifest = entry.path().join("manifest.json");
        let Ok(contents) = fs::read_to_string(manifest) else {
            continue;
        };
        let Ok(record) = serde_json::from_str::<TransactionRecord>(&contents) else {
            continue;
        };
        records.push(record);
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
