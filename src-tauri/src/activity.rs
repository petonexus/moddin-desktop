use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    env, fs,
    fs::OpenOptions,
    io::Write,
    path::PathBuf,
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_LOG_FILE_BYTES: u64 = 5 * 1024 * 1024;
const DEFAULT_LIST_LIMIT: usize = 200;
const MAX_LIST_LIMIT: usize = 1_000;
const MAX_DETAILS: usize = 32;
const MAX_REFERENCE_BYTES: usize = 128;
const MAX_MESSAGE_BYTES: usize = 2_000;
const MAX_DETAIL_KEY_BYTES: usize = 128;
const MAX_DETAIL_VALUE_BYTES: usize = 1_024;

static LOG_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionLogEntry {
    pub id: String,
    pub timestamp: u64,
    pub level: String,
    pub action: String,
    pub game_id: Option<String>,
    pub transaction_id: Option<String>,
    pub message: String,
    #[serde(default)]
    pub details: BTreeMap<String, String>,
}

fn now_millis() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .map_err(|error| format!("System clock error while writing action log: {error}"))
}

fn logs_root() -> PathBuf {
    env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir)
        .join("Moddin")
        .join("logs")
}

fn current_log_path() -> PathBuf {
    logs_root().join("actions.jsonl")
}

fn rotated_log_path() -> PathBuf {
    logs_root().join("actions.1.jsonl")
}

fn should_rotate(current_bytes: u64, incoming_bytes: u64) -> bool {
    current_bytes.saturating_add(incoming_bytes) > MAX_LOG_FILE_BYTES
}

fn rotate_if_needed(path: &PathBuf, incoming_bytes: u64) -> Result<(), String> {
    let Ok(metadata) = fs::metadata(path) else {
        return Ok(());
    };
    if !should_rotate(metadata.len(), incoming_bytes) {
        return Ok(());
    }

    let rotated = rotated_log_path();
    if rotated.exists() {
        fs::remove_file(&rotated)
            .map_err(|error| format!("Could not remove previous rotated action log: {error}"))?;
    }
    fs::rename(path, &rotated)
        .map_err(|error| format!("Could not rotate action log: {error}"))
}

pub fn record(
    level: &str,
    action: &str,
    game_id: Option<&str>,
    transaction_id: Option<&str>,
    message: impl Into<String>,
    details: BTreeMap<String, String>,
) -> Result<ActionLogEntry, String> {
    let _guard = LOG_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "Action log lock is poisoned.".to_owned())?;

    let root = logs_root();
    fs::create_dir_all(&root)
        .map_err(|error| format!("Could not create Moddin action log directory: {error}"))?;

    let entry = ActionLogEntry {
        id: uuid::Uuid::new_v4().to_string(),
        timestamp: now_millis()?,
        level: level.to_owned(),
        action: action.to_owned(),
        game_id: game_id.map(str::to_owned),
        transaction_id: transaction_id.map(str::to_owned),
        message: message.into(),
        details,
    };

    let serialized = serde_json::to_string(&entry)
        .map_err(|error| format!("Could not serialize action log entry: {error}"))?;
    let incoming_bytes = serialized.len() as u64 + 1;
    if incoming_bytes > MAX_LOG_FILE_BYTES {
        return Err("Action log entry exceeds the maximum log file size.".to_owned());
    }

    let path = current_log_path();
    rotate_if_needed(&path, incoming_bytes)?;

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|error| format!("Could not open Moddin action log: {error}"))?;
    writeln!(file, "{serialized}")
        .map_err(|error| format!("Could not append Moddin action log: {error}"))?;

    Ok(entry)
}

fn read_log_file(path: PathBuf, entries: &mut Vec<ActionLogEntry>) {
    let Ok(contents) = fs::read_to_string(path) else {
        return;
    };

    for line in contents.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(entry) = serde_json::from_str::<ActionLogEntry>(line) {
            entries.push(entry);
        }
    }
}

fn valid_level(level: &str) -> bool {
    matches!(level, "info" | "success" | "warning" | "error")
}

fn valid_action(action: &str) -> bool {
    !action.is_empty()
        && action.len() <= 96
        && action
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.'))
}

fn valid_reference(reference: &str) -> bool {
    !reference.is_empty()
        && reference.len() <= MAX_REFERENCE_BYTES
        && reference
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.'))
}

fn valid_optional_reference(reference: Option<&str>) -> bool {
    reference.is_none_or(valid_reference)
}

#[tauri::command]
pub fn record_ui_action_log(
    level: String,
    action: String,
    game_id: Option<String>,
    transaction_id: Option<String>,
    message: String,
    details: Option<BTreeMap<String, String>>,
) -> Result<ActionLogEntry, String> {
    if !valid_level(&level) {
        return Err("Invalid action log level.".to_owned());
    }
    if !valid_action(&action) {
        return Err("Invalid action log action name.".to_owned());
    }
    if !valid_optional_reference(game_id.as_deref()) {
        return Err("Invalid action log game id.".to_owned());
    }
    if !valid_optional_reference(transaction_id.as_deref()) {
        return Err("Invalid action log transaction id.".to_owned());
    }
    if message.len() > MAX_MESSAGE_BYTES {
        return Err("Action log message is too large.".to_owned());
    }

    let details = details.unwrap_or_default();
    if details.len() > MAX_DETAILS
        || details.iter().any(|(key, value)| {
            key.len() > MAX_DETAIL_KEY_BYTES || value.len() > MAX_DETAIL_VALUE_BYTES
        })
    {
        return Err("Action log details exceed the allowed size.".to_owned());
    }

    record(
        &level,
        &action,
        game_id.as_deref(),
        transaction_id.as_deref(),
        message,
        details,
    )
}

#[tauri::command]
pub fn list_action_logs(limit: Option<usize>) -> Result<Vec<ActionLogEntry>, String> {
    let _guard = LOG_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "Action log lock is poisoned.".to_owned())?;

    let requested_limit = limit
        .unwrap_or(DEFAULT_LIST_LIMIT)
        .clamp(1, MAX_LIST_LIMIT);
    let mut entries = Vec::new();
    read_log_file(rotated_log_path(), &mut entries);
    read_log_file(current_log_path(), &mut entries);
    entries.sort_by(|left, right| right.timestamp.cmp(&left.timestamp));
    entries.truncate(requested_limit);
    Ok(entries)
}

#[tauri::command]
pub fn clear_action_logs() -> Result<(), String> {
    let _guard = LOG_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "Action log lock is poisoned.".to_owned())?;

    for path in [current_log_path(), rotated_log_path()] {
        if path.exists() {
            fs::remove_file(&path)
                .map_err(|error| format!("Could not remove action log '{}': {error}", path.display()))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_entry_round_trips_as_json() {
        let mut details = BTreeMap::new();
        details.insert("proxy".to_owned(), "dxgi.dll".to_owned());
        let entry = ActionLogEntry {
            id: "test-id".to_owned(),
            timestamp: 42,
            level: "success".to_owned(),
            action: "optiscaler".to_owned(),
            game_id: Some("cyberpunk-2077".to_owned()),
            transaction_id: Some("tx".to_owned()),
            message: "Installed".to_owned(),
            details,
        };

        let json = serde_json::to_string(&entry).unwrap();
        let decoded: ActionLogEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.action, "optiscaler");
        assert_eq!(decoded.details.get("proxy").map(String::as_str), Some("dxgi.dll"));
    }

    #[test]
    fn validates_levels_action_names_and_references() {
        assert!(valid_level("success"));
        assert!(!valid_level("trace"));
        assert!(valid_action("set_system_openxr_runtime"));
        assert!(!valid_action("../escape"));
        assert!(valid_reference("cyberpunk-2077"));
        assert!(valid_reference("1757720000000-0123456789abcdef0123456789abcdef"));
        assert!(!valid_reference("../escape"));
        assert!(!valid_reference(&"x".repeat(MAX_REFERENCE_BYTES + 1)));
    }

    #[test]
    fn rotation_boundary_includes_the_next_entry() {
        assert!(!should_rotate(0, MAX_LOG_FILE_BYTES));
        assert!(!should_rotate(MAX_LOG_FILE_BYTES - 10, 10));
        assert!(should_rotate(MAX_LOG_FILE_BYTES - 10, 11));
        assert!(should_rotate(u64::MAX, 1));
    }
}
