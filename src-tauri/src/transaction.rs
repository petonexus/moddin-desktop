use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

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
    let output = Command::new("tasklist")
        .args(["/FI", &format!("IMAGENAME eq {image_name}"), "/FO", "CSV", "/NH"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();

    let Ok(output) = output else {
        return false;
    };

    String::from_utf8_lossy(&output.stdout)
        .to_ascii_lowercase()
        .contains(&image_name.to_ascii_lowercase())
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
        return Err("Windows could not request a graceful OBS shutdown before rollback.".to_owned());
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

pub fn backup_file(
    target: &Path,
    kind: &str,
    label: &str,
    game_id: &str,
) -> Result<TransactionRecord, String> {
    if !target.is_file() {
        return Err(format!("Cannot back up missing file: {}", target.display()));
    }

    let created_at = now_millis()?;
    let id = format!("{}-{}", created_at, uuid::Uuid::new_v4().simple());
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
    };

    write_record(&record)?;
    Ok(record)
}

pub fn restore_record(mut record: TransactionRecord) -> Result<TransactionRecord, String> {
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

    record.status = "rolled_back".to_owned();
    write_record(&record)?;
    Ok(record)
}

#[tauri::command]
pub fn list_transactions() -> Result<Vec<TransactionRecord>, String> {
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
pub fn rollback_transaction(id: String) -> Result<TransactionRecord, String> {
    let record = read_record(&id)?;
    if record.status == "rolled_back" {
        return Ok(record);
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
