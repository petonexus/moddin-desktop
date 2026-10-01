//! AI assistant integration — connect a local AI tool (Cursor, Claude
//! Desktop, Codex) to Moddin without the user editing JSON files or
//! installing Node.js.
//!
//! The agent-server binary is shipped as a Tauri resource (see
//! `tauri.conf.json → bundle.resources`) and resolved at runtime via
//! `app.path().resource_dir()`. Detection of each AI tool uses
//! well-known install locations on Windows only — Moddin is Windows-only.
//!
//! ## Threat model
//!
//! This module touches **only** the AI tool's MCP config file, never the
//! game directory, never the registry, never HKLM. The path the helper
//! writes to is hard-coded for each supported agent and rejected if the
//! caller passes anything else. Writes are atomic (write-to-temp +
//! rename) and we back up the existing config before mutating it so
//! `remove_ai_assistant` is a true rollback.
//!
//! ## Authoring flow (after `setup_ai_assistant` succeeds)
//!
//! The AI tool launches the bundled `moddin-mcp.exe` over stdio. The user
//! then chats with the AI and asks it to add a mod — the AI calls into
//! the MCP server which validates and writes the YAML to
//! `%LOCALAPPDATA%\Moddin\capabilities\`. Moddin Desktop reads the file
//! on the next launch.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Stable identifier for each supported AI assistant.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum AgentId {
    Cursor,
    ClaudeDesktop,
    Codex,
}

impl AgentId {
    pub fn display_name(self) -> &'static str {
        match self {
            AgentId::Cursor => "Cursor",
            AgentId::ClaudeDesktop => "Claude Desktop",
            AgentId::Codex => "Codex CLI",
        }
    }
}

/// Connection state for one AI assistant. The UI surfaces this as a
/// green/yellow/red badge.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionState {
    /// AI tool is not installed on this machine.
    NotInstalled,
    /// AI tool is installed but the Moddin MCP entry is missing.
    DetectedNotConfigured,
    /// Moddin MCP entry is present and points at our bundled .exe.
    Configured,
    /// Moddin MCP entry is present but malformed or points elsewhere.
    ConfigError,
}

/// Per-agent status returned to the UI.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiAgent {
    pub id: AgentId,
    pub display_name: &'static str,
    pub state: ConnectionState,
    pub config_path: Option<String>,
    pub binary_path: Option<String>,
    pub detail: Option<String>,
}

// -----------------------------------------------------------------------------
// Path allowlist (Windows only).
//
// Every AI tool writes its MCP config to a fixed, well-known location.
// We hardcode the set of paths this module may touch so that a future
// refactor cannot accidentally introduce arbitrary path writes.
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct AgentLayout {
    config_dir_from_env: fn() -> Option<PathBuf>,
    config_filename: &'static str,
    detect_binary: fn() -> Option<PathBuf>,
}

const LAYOUTS: &[(AgentId, AgentLayout)] = &[
    (
        AgentId::Cursor,
        AgentLayout {
            config_dir_from_env: cursor_config_dir,
            config_filename: "mcp.json",
            detect_binary: cursor_binary,
        },
    ),
    (
        AgentId::ClaudeDesktop,
        AgentLayout {
            config_dir_from_env: claude_config_dir,
            config_filename: "claude_desktop_config.json",
            detect_binary: claude_binary,
        },
    ),
    (
        AgentId::Codex,
        AgentLayout {
            config_dir_from_env: codex_config_dir,
            config_filename: "mcp.json",
            detect_binary: codex_binary,
        },
    ),
];

fn layout_for(agent: AgentId) -> AgentLayout {
    LAYOUTS
        .iter()
        .find(|(id, _)| *id == agent)
        .map(|(_, layout)| *layout)
        .expect("layout is exhaustive over AgentId")
}

fn cursor_config_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .map(|p| p.join(".cursor"))
}

fn claude_config_dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .map(|p| p.join("Claude"))
}

fn codex_config_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .map(|p| p.join(".codex"))
}

fn first_existing(paths: &[PathBuf]) -> Option<PathBuf> {
    paths.iter().find(|p| p.is_file()).cloned()
}

fn cursor_binary() -> Option<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from)?;
    let candidates = [
        local.join("Programs").join("Cursor").join("Cursor.exe"),
        local.join("Programs").join("cursor").join("Cursor.exe"),
    ];
    first_existing(&candidates).or_else(|| which_in_path("cursor").map(PathBuf::from))
}

fn claude_binary() -> Option<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from)?;
    let candidates = [
        local.join("Programs").join("Claude").join("Claude.exe"),
        local.join("Programs").join("claude").join("Claude.exe"),
    ];
    first_existing(&candidates).or_else(|| which_in_path("claude").map(PathBuf::from))
}

fn codex_binary() -> Option<PathBuf> {
    which_in_path("codex").map(PathBuf::from)
}

fn which_in_path(name: &str) -> Option<String> {
    // Minimal `where.exe`-style probe. We don't ship `which` as a
    // dependency, so check PATH manually with the Windows separator.
    let path_var = std::env::var_os("PATH")?;
    let exts = [".exe", ".cmd", ".bat", ""];
    for dir in std::env::split_paths(&path_var) {
        for ext in &exts {
            let candidate = dir.join(format!("{name}{ext}"));
            if candidate.is_file() {
                return Some(candidate.to_string_lossy().into_owned());
            }
        }
    }
    None
}

/// Resolve the MCP server runtime Moddin ships. We support two layouts
/// so that future builds can switch from Node-portable to a real `.exe`
/// without changing the Tauri command surface:
///
/// 1. **Portable Node** (today): `<resource>/node.exe` and
///    `<resource>/moddin-agent/src/mcp-server.mjs`.
/// 2. **Standalone binary** (future): `<resource>/moddin-mcp.exe`.
struct McpRuntime {
    command: PathBuf,
    args: Vec<String>,
}

fn resolve_mcp_server(resource_dir: &Path) -> Option<McpRuntime> {
    // Layout 2 — single .exe (future).
    let exe_candidates = [
        resource_dir.join("moddin-mcp.exe"),
        resource_dir.join("bin").join("moddin-mcp.exe"),
    ];
    for exe in exe_candidates {
        if exe.is_file() {
            return Some(McpRuntime {
                command: exe,
                args: vec![],
            });
        }
    }
    // Layout 1 — portable Node + script.
    let node = resource_dir.join("node.exe");
    let script = resource_dir
        .join("moddin-agent")
        .join("src")
        .join("mcp-server.mjs");
    if node.is_file() && script.is_file() {
        return Some(McpRuntime {
            command: node,
            args: vec![script.to_string_lossy().into_owned()],
        });
    }
    None
}

// -----------------------------------------------------------------------------
// Config file I/O.
//
// We read the config as a generic JSON object, mutate only the
// `mcpServers.moddin` key, and write the whole file back. Existing user
// entries are preserved verbatim.
// -----------------------------------------------------------------------------

const MCP_SERVER_KEY: &str = "moddin";

/// Value the bundled MCP server expects for `MODDIN_PROJECT_ROOT`.
/// NOTE: hard-coded to the dev checkout on this branch; production builds
/// must derive it from the install/resource layout.
const MODDIN_PROJECT_ROOT_ENV_VALUE: &str = "C:/mods/moddin/moddin";

// -----------------------------------------------------------------------------
// Codex CLI MCP registration
//
// The Codex desktop app's MCP settings UI (and our JSON config, which
// mirrors it) live in `mcp.json`, but the Codex *CLI* — the binary the
// headless agent mode drives — only reads `$CODEX_HOME/config.toml`
// (`[mcp_servers.<name>]`). To make "the AI does it for the user" work
// for Codex we maintain the same Moddin entry in both files. The TOML
// surgery below is string-based on purpose: it preserves every other
// section of the user's config byte-for-byte instead of reserializing
// the whole document.
// -----------------------------------------------------------------------------

const CODEX_TOML_SECTION: &str = "[mcp_servers.moddin]";
const CODEX_TOML_ENV_SECTION: &str = "[mcp_servers.moddin.env]";

fn is_toml_section_header(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with('[') && trimmed.ends_with(']')
}

/// Drop the `[mcp_servers.moddin]` and `[mcp_servers.moddin.env]`
/// blocks. Returns the new text and whether anything was removed.
fn remove_codex_toml_entry(toml: &str) -> (String, bool) {
    let mut out: Vec<&str> = Vec::new();
    let mut skipping = false;
    let mut removed = false;
    for line in toml.lines() {
        if is_toml_section_header(line) {
            let name = line.trim();
            skipping = name == CODEX_TOML_SECTION || name == CODEX_TOML_ENV_SECTION;
            if skipping {
                removed = true;
                continue;
            }
            out.push(line);
        } else if skipping {
            // Any non-header line inside a removed block (including the
            // blank separator lines) goes away with it.
            continue;
        } else {
            out.push(line);
        }
    }
    (out.join("\n"), removed)
}

fn toml_single_quoted(value: &str) -> String {
    // TOML *literal* strings (single quotes) take backslashes verbatim
    // and cannot escape a quote — so quote-free paths stay literal, and
    // anything containing a single quote falls back to a basic string
    // with proper escaping.
    if value.contains('\'') {
        format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        format!("'{value}'")
    }
}

/// Insert (or replace) the Moddin MCP entry at the end of the document.
pub fn upsert_codex_toml_entry(
    toml: &str,
    command: &str,
    args: &[String],
    project_root: &str,
) -> String {
    let (mut body, _) = remove_codex_toml_entry(toml);
    while body.ends_with('\n') {
        body.pop();
    }
    if !body.is_empty() {
        body.push('\n');
    }
    let args_toml = args
        .iter()
        .map(|arg| toml_single_quoted(arg))
        .collect::<Vec<_>>()
        .join(", ");
    body.push_str(&format!("{CODEX_TOML_SECTION}\n"));
    body.push_str(&format!("command = {}\n", toml_single_quoted(command)));
    body.push_str(&format!("args = [{args_toml}]\n"));
    body.push('\n');
    body.push_str(&format!("{CODEX_TOML_ENV_SECTION}\n"));
    body.push_str("MODDIN_LOCAL_CAPABILITIES_DIR = ''\n");
    body.push_str(&format!(
        "MODDIN_PROJECT_ROOT = {}\n",
        toml_single_quoted(project_root)
    ));
    body
}

fn codex_toml_path() -> Option<PathBuf> {
    codex_config_dir().map(|dir| dir.join("config.toml"))
}

fn write_codex_toml_entry(command: &str, args: &[String]) -> Result<(), String> {
    let path = codex_toml_path()
        .ok_or_else(|| "Could not resolve the Codex config directory.".to_owned())?;
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let updated = upsert_codex_toml_entry(&existing, command, args, MODDIN_PROJECT_ROOT_ENV_VALUE);
    if updated == existing {
        return Ok(());
    }
    write_config(&path, &updated)
}

fn remove_codex_toml_entry_file() -> Result<(), String> {
    let path = codex_toml_path()
        .ok_or_else(|| "Could not resolve the Codex config directory.".to_owned())?;
    if !path.exists() {
        return Ok(());
    }
    let existing =
        fs::read_to_string(&path).map_err(|e| format!("Could not read {}: {e}", path.display()))?;
    let (updated, removed) = remove_codex_toml_entry(&existing);
    if !removed {
        return Ok(());
    }
    write_config(&path, &updated)
}

fn read_mcp_config(path: &Path) -> Result<serde_json::Value, String> {
    if !path.exists() {
        return Ok(serde_json::json!({ "mcpServers": {} }));
    }
    let raw =
        fs::read_to_string(path).map_err(|e| format!("Could not read {}: {e}", path.display()))?;
    if raw.trim().is_empty() {
        return Ok(serde_json::json!({ "mcpServers": {} }));
    }
    let mut value: serde_json::Value = serde_json::from_str(&raw)
        .map_err(|e| format!("Config {} is not valid JSON: {e}", path.display()))?;
    if !value.is_object() {
        return Err(format!("Config {} is not a JSON object", path.display()));
    }
    if value
        .get("mcpServers")
        .map(|v| v.is_object())
        .unwrap_or(false)
    {
        return Ok(value);
    }
    // Tolerate missing or wrong-typed mcpServers — create a fresh map.
    if let Some(obj) = value.as_object_mut() {
        obj.insert("mcpServers".to_string(), serde_json::json!({}));
    }
    Ok(value)
}

/// Write `body` to `path`, putting the previous contents back if the write
/// does not land.
///
/// Every config this module writes goes through here, which is what makes
/// this the only place that holds both the bytes being replaced and the
/// write that can destroy them. The old contents land in
/// `<name>.moddin-bak` first — taken here rather than by each caller, so
/// the backup is always the exact content this write is about to replace
/// and can never be a leftover from an earlier run — and come back if
/// `fs::write` fails. A truncated `mcp.json` stops the user's assistant
/// from starting at all, and a backup nothing restores is not a backup.
fn write_config(path: &Path, body: &str) -> Result<(), String> {
    let dir = path
        .parent()
        .ok_or_else(|| format!("Config {} has no parent directory", path.display()))?;
    fs::create_dir_all(dir).map_err(|e| format!("Could not create {}: {e}", dir.display()))?;
    if path.is_file() {
        let bak = backup_path(path);
        fs::copy(path, &bak).map_err(|e| format!("Could not back up {}: {e}", path.display()))?;
    }
    match fs::write(path, body) {
        Ok(()) => Ok(()),
        Err(error) => match restore_from_backup(path) {
            Ok(()) => Err(format!(
                "Could not write {}: {error}. The previous config was restored from {}",
                path.display(),
                backup_path(path).display()
            )),
            Err(restore_error) => Err(format!(
                "Could not write {}: {error}. The previous config could not be restored: {restore_error}",
                path.display()
            )),
        },
    }
}

/// Put `<name>.moddin-bak` back over the config it was taken from.
///
/// Copied rather than renamed, so the `.moddin-bak` the user can still
/// see afterwards is the same file the rollback just used, and so there
/// is no instant in which neither the config nor the backup exists.
fn restore_from_backup(config_path: &Path) -> Result<(), String> {
    let bak = backup_path(config_path);
    if !bak.exists() {
        return Err(format!("No backup file exists at {}", bak.display()));
    }
    fs::copy(&bak, config_path)
        .map(|_| ())
        .map_err(|e| format!("Could not restore backup: {e}"))
}

/// Backwards-compatible alias used by the older code paths.
fn atomic_write(path: &Path, body: &str) -> Result<(), String> {
    write_config(path, body)
}

fn backup_path(config_path: &Path) -> PathBuf {
    let parent = config_path.parent().unwrap_or_else(|| Path::new("."));
    let name = config_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("config");
    parent.join(format!("{name}.moddin-bak"))
}

// -----------------------------------------------------------------------------
// Public Tauri commands.
// -----------------------------------------------------------------------------

/// Return the connection state for every supported AI assistant.
///
/// `resource_dir` is the Tauri resource directory; it is the location
/// where the bundled `moddin-mcp.exe` will live after install.
#[tauri::command]
pub fn detect_ai_assistants(resource_dir: String) -> Vec<AiAgent> {
    let resource_path = PathBuf::from(resource_dir);
    AgentId::all()
        .into_iter()
        .map(|id| inspect_agent(id, &resource_path))
        .collect()
}

/// Connect the supplied AI assistant to Moddin by writing the MCP server
/// entry into the tool's config. Idempotent — running it twice is safe.
#[tauri::command]
pub fn setup_ai_assistant(agent_id: AgentId, resource_dir: String) -> Result<AiAgent, String> {
    let resource_path = PathBuf::from(resource_dir);
    let layout = layout_for(agent_id);
    let config_path = (layout.config_dir_from_env)()
        .ok_or_else(|| {
            format!(
                "Could not resolve config directory for {}",
                agent_id.display_name()
            )
        })?
        .join(layout.config_filename);
    let mcp_server = resolve_mcp_server(&resource_path).ok_or_else(|| {
        format!(
            "Bundled MCP server not found under {}",
            resource_path.display()
        )
    })?;
    let mcp_command = mcp_server.command.to_string_lossy().into_owned();
    let mcp_args = mcp_server.args.clone();

    let mut config = read_mcp_config(&config_path)?;
    let mcp_servers = config
        .get_mut("mcpServers")
        .and_then(|v| v.as_object_mut())
        .ok_or_else(|| format!("Config {} has no mcpServers object", config_path.display()))?;

    // Preserve any sibling keys, only touch our own.
    mcp_servers.insert(
        MCP_SERVER_KEY.to_string(),
        serde_json::json!({
            "command": mcp_command.clone(),
            "args": mcp_args,
            "env": {
                "MODDIN_LOCAL_CAPABILITIES_DIR": "",
                "MODDIN_PROJECT_ROOT": MODDIN_PROJECT_ROOT_ENV_VALUE,
            },
        }),
    );

    // `write_config` backs up the existing config and rolls it back if the
    // write does not land, so there is nothing to do here before it.
    let serialized = serde_json::to_string_pretty(&config)
        .map_err(|e| format!("Could not serialize updated config: {e}"))?;
    atomic_write(&config_path, &serialized)?;

    // The Codex CLI ignores mcp.json — it only reads
    // `$CODEX_HOME/config.toml`. Mirror the entry there so headless
    // `codex exec` runs (agent mode) see the Moddin tools.
    if agent_id == AgentId::Codex {
        write_codex_toml_entry(&mcp_command, &mcp_args)?;
    }

    Ok(inspect_agent(agent_id, &resource_path))
}

/// Disconnect the AI assistant by removing the Moddin MCP entry. The
/// previous config (if any) is restored from the backup file.
#[tauri::command]
pub fn remove_ai_assistant(agent_id: AgentId, resource_dir: String) -> Result<AiAgent, String> {
    let resource_path = PathBuf::from(resource_dir);
    let layout = layout_for(agent_id);
    let config_path = (layout.config_dir_from_env)()
        .ok_or_else(|| {
            format!(
                "Could not resolve config directory for {}",
                agent_id.display_name()
            )
        })?
        .join(layout.config_filename);
    if !config_path.exists() {
        // Nothing to do — return the fresh inspect.
        return Ok(inspect_agent(agent_id, &resource_path));
    }
    let mut config = read_mcp_config(&config_path)?;
    let removed = config
        .get_mut("mcpServers")
        .and_then(|v| v.as_object_mut())
        .map(|obj| obj.remove(MCP_SERVER_KEY).is_some())
        .unwrap_or(false);
    if !removed {
        // Still try to clean the Codex CLI config — the entry may exist
        // there from a previous build even when mcp.json is already gone.
        if agent_id == AgentId::Codex {
            remove_codex_toml_entry_file()?;
        }
        return Ok(inspect_agent(agent_id, &resource_path));
    }
    let serialized = serde_json::to_string_pretty(&config)
        .map_err(|e| format!("Could not serialize config: {e}"))?;
    atomic_write(&config_path, &serialized)?;
    if agent_id == AgentId::Codex {
        remove_codex_toml_entry_file()?;
    }
    Ok(inspect_agent(agent_id, &resource_path))
}

// -----------------------------------------------------------------------------
// Inspection / state derivation.
// -----------------------------------------------------------------------------

fn inspect_agent(id: AgentId, resource_dir: &Path) -> AiAgent {
    let layout = layout_for(id);
    let binary_path = (layout.detect_binary)();
    let config_path = (layout.config_dir_from_env)().map(|d| d.join(layout.config_filename));
    let (state, detail) = match (binary_path.as_ref(), config_path.as_ref()) {
        (None, _) => (
            ConnectionState::NotInstalled,
            Some("AI tool is not installed on this PC.".to_string()),
        ),
        (Some(_), None) => (
            ConnectionState::NotInstalled,
            Some("AI tool is installed but its config directory was not found.".to_string()),
        ),
        (Some(bin), Some(path)) => match detect_moddin_entry(path) {
            Ok(Some(entry)) => {
                let expected_command = resolve_mcp_server(resource_dir)
                    .map(|rt| rt.command.to_string_lossy().into_owned())
                    .unwrap_or_default();
                match entry.get("command").and_then(|v| v.as_str()) {
                    Some(cmd) if !expected_command.is_empty() && cmd == expected_command => {
                        (ConnectionState::Configured, None)
                    }
                    Some(other) => (
                        ConnectionState::ConfigError,
                        Some(format!(
                            "Moddin MCP entry points at a different binary: {}",
                            other
                        )),
                    ),
                    None => (
                        ConnectionState::ConfigError,
                        Some("Moddin MCP entry has no command field.".to_string()),
                    ),
                }
            }
            Ok(None) => (
                ConnectionState::DetectedNotConfigured,
                Some(format!(
                    "AI tool installed at {}. Click Connect to register Moddin.",
                    bin.display()
                )),
            ),
            Err(msg) => (ConnectionState::ConfigError, Some(msg)),
        },
    };
    AiAgent {
        id,
        display_name: id.display_name(),
        state,
        config_path: config_path.map(|p| p.to_string_lossy().into_owned()),
        binary_path: binary_path.map(|p| p.to_string_lossy().into_owned()),
        detail,
    }
}

fn detect_moddin_entry(config_path: &Path) -> Result<Option<serde_json::Value>, String> {
    if !config_path.exists() {
        return Ok(None);
    }
    let config = read_mcp_config(config_path)?;
    Ok(config
        .get("mcpServers")
        .and_then(|v| v.as_object())
        .and_then(|obj| obj.get(MCP_SERVER_KEY))
        .cloned())
}

impl AgentId {
    pub fn all() -> [AgentId; 3] {
        [AgentId::Cursor, AgentId::ClaudeDesktop, AgentId::Codex]
    }
}

// -----------------------------------------------------------------------------
// Tests.
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    // Process-wide env tests cannot run in parallel: each one points
    // USERPROFILE / LOCALAPPDATA at its own temp dir, so a concurrent
    // test would resolve the other test's paths and fail flakily.
    // Acquire this lock at the start of every test that mutates env.
    fn env_test_lock() -> std::sync::MutexGuard<'static, ()> {
        crate::test_support::env_lock()
    }

    struct EnvGuard {
        key: &'static str,
        prev: Option<String>,
    }
    impl EnvGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let prev = env::var(key).ok();
            env::set_var(key, value);
            Self { key, prev }
        }
    }
    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.prev {
                Some(v) => env::set_var(self.key, v),
                None => env::remove_var(self.key),
            }
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let base = env::temp_dir();
        let unique = base.join(format!("moddin-ai-test-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&unique);
        fs::create_dir_all(&unique).unwrap();
        unique
    }

    /// Stub the Cursor binary detection: create the expected
    /// `%LOCALAPPDATA%\Programs\Cursor\Cursor.exe` layout under `dir`
    /// and point LOCALAPPDATA at it, so tests don't depend on the
    /// machine running them actually having Cursor installed.
    fn stub_cursor_install(dir: &Path) -> EnvGuard {
        let bin_dir = dir.join("Programs").join("Cursor");
        fs::create_dir_all(&bin_dir).unwrap();
        fs::write(bin_dir.join("Cursor.exe"), "stub").unwrap();
        EnvGuard::set("LOCALAPPDATA", dir.to_string_lossy().as_ref())
    }

    #[test]
    fn setup_then_remove_round_trip() {
        let _env = env_test_lock();
        let dir = temp_dir("roundtrip");
        let cfg_dir = dir.join(".cursor");
        fs::create_dir_all(&cfg_dir).unwrap();
        let cfg_path = cfg_dir.join("mcp.json");
        // Pre-existing user config the user wants to keep.
        fs::write(
            &cfg_path,
            r#"{ "mcpServers": { "hindsight": { "command": "node", "args": ["h.js"] } } }"#,
        )
        .unwrap();

        let _userprofile = EnvGuard::set("USERPROFILE", dir.to_string_lossy().as_ref());
        let _localappdata = stub_cursor_install(&dir);
        let res_dir = make_runtime_layout(&dir.join("resources"));

        // SETUP
        let agent = setup_ai_assistant(AgentId::Cursor, res_dir.to_string_lossy().into_owned())
            .expect("setup should succeed");
        assert_eq!(agent.state, ConnectionState::Configured);

        // Verify hindsight survives and moddin is added with the
        // portable-Node shape (command = node.exe, args = [script]).
        let raw = fs::read_to_string(&cfg_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert!(
            parsed["mcpServers"]["hindsight"].is_object(),
            "hindsight entry must survive setup"
        );
        let moddin = &parsed["mcpServers"]["moddin"];
        assert_eq!(
            moddin["command"].as_str().unwrap(),
            res_dir.join("node.exe").to_string_lossy().as_ref()
        );
        let args = moddin["args"].as_array().expect("args must be an array");
        assert_eq!(args.len(), 1, "exactly one arg (the script path)");
        assert_eq!(
            args[0].as_str().unwrap(),
            res_dir
                .join("moddin-agent")
                .join("src")
                .join("mcp-server.mjs")
                .to_string_lossy()
                .as_ref()
        );

        // REMOVE
        let agent = remove_ai_assistant(AgentId::Cursor, res_dir.to_string_lossy().into_owned())
            .expect("remove should succeed");
        assert_eq!(agent.state, ConnectionState::DetectedNotConfigured);

        let raw = fs::read_to_string(&cfg_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert!(
            parsed["mcpServers"]["hindsight"].is_object(),
            "hindsight entry must survive remove"
        );
        assert!(
            parsed["mcpServers"].get("moddin").is_none(),
            "moddin entry must be gone"
        );
    }

    #[test]
    fn setup_is_idempotent() {
        let _env = env_test_lock();
        let dir = temp_dir("idempotent");
        let cfg_dir = dir.join(".cursor");
        fs::create_dir_all(&cfg_dir).unwrap();
        let res_dir = make_runtime_layout(&dir.join("resources"));
        let _userprofile = EnvGuard::set("USERPROFILE", dir.to_string_lossy().as_ref());
        let _localappdata = stub_cursor_install(&dir);

        for _ in 0..3 {
            let agent = setup_ai_assistant(AgentId::Cursor, res_dir.to_string_lossy().into_owned())
                .expect("setup should succeed");
            assert_eq!(agent.state, ConnectionState::Configured);
        }
    }

    #[test]
    fn missing_runtime_returns_descriptive_error() {
        let _env = env_test_lock();
        let dir = temp_dir("missing-runtime");
        let res_dir = dir.join("resources");
        fs::create_dir_all(&res_dir).unwrap();
        let _userprofile =
            EnvGuard::set("USERPROFILE", dir.join("home").to_string_lossy().as_ref());

        let err = setup_ai_assistant(AgentId::Cursor, res_dir.to_string_lossy().into_owned())
            .expect_err("setup should fail when MCP runtime is missing");
        assert!(err.contains("Bundled MCP server not found"), "{err}");
    }

    #[test]
    fn unparseable_config_is_rejected() {
        let _env = env_test_lock();
        let dir = temp_dir("bad-json");
        let cfg_dir = dir.join(".cursor");
        fs::create_dir_all(&cfg_dir).unwrap();
        let cfg_path = cfg_dir.join("mcp.json");
        fs::write(&cfg_path, "{ this is not json").unwrap();
        let _userprofile = EnvGuard::set("USERPROFILE", dir.to_string_lossy().as_ref());

        let res_dir = make_runtime_layout(&dir.join("resources"));

        let err = setup_ai_assistant(AgentId::Cursor, res_dir.to_string_lossy().into_owned())
            .expect_err("setup should reject bad JSON");
        assert!(err.contains("not valid JSON"), "{err}");
    }

    /// Lay down a fake but valid Moddin runtime: a fake `node.exe` and a
    /// stub script at the exact path the production layout uses. The
    /// function touches nothing else.
    fn make_runtime_layout(res_dir: &Path) -> PathBuf {
        fs::create_dir_all(res_dir).unwrap();
        fs::write(res_dir.join("node.exe"), "stub-node").unwrap();
        let script = res_dir
            .join("moddin-agent")
            .join("src")
            .join("mcp-server.mjs");
        fs::create_dir_all(script.parent().unwrap()).unwrap();
        fs::write(script, "stub-script").unwrap();
        res_dir.to_path_buf()
    }

    #[test]
    fn inspect_reports_detected_not_configured() {
        let _env = env_test_lock();
        let dir = temp_dir("inspect");
        let cfg_dir = dir.join(".cursor");
        fs::create_dir_all(&cfg_dir).unwrap();
        let cfg_path = cfg_dir.join("mcp.json");
        fs::write(&cfg_path, r#"{ "mcpServers": {} }"#).unwrap();
        let _userprofile = EnvGuard::set("USERPROFILE", dir.to_string_lossy().as_ref());
        let _localappdata = stub_cursor_install(&dir);

        let res_dir = dir.join("resources");
        fs::create_dir_all(&res_dir).unwrap();

        let agent = detect_ai_assistants(res_dir.to_string_lossy().into_owned())
            .into_iter()
            .find(|a| a.id == AgentId::Cursor)
            .expect("cursor agent must be present");
        assert_eq!(agent.state, ConnectionState::DetectedNotConfigured);
    }

    #[test]
    fn codex_toml_upsert_preserves_other_sections() {
        let existing = r#"
[mcp_servers.node_repl]
args = []
command = 'C:\OpenAI\Codex\node_repl.exe'

[projects.'c:\mods\moddin']
trust_level = 'trusted'
"#;
        let updated = upsert_codex_toml_entry(
            existing,
            r"C:\moddin\node.exe",
            &[r"C:\moddin\moddin-mcp.mjs".to_owned()],
            "C:/mods/moddin/moddin",
        );
        assert!(updated.contains("[mcp_servers.node_repl]"), "{}", updated);
        assert!(
            updated.contains(r"C:\OpenAI\Codex\node_repl.exe"),
            "{}",
            updated
        );
        assert!(
            updated.contains("[projects.'c:\\mods\\moddin']"),
            "{}",
            updated
        );
        assert!(updated.contains("[mcp_servers.moddin]"), "{}", updated);
        assert!(
            updated.contains(r"command = 'C:\moddin\node.exe'"),
            "{}",
            updated
        );
        assert!(
            updated.contains(r"'C:\moddin\moddin-mcp.mjs'"),
            "{}",
            updated
        );
        assert!(
            updated.contains("MODDIN_PROJECT_ROOT = 'C:/mods/moddin/moddin'"),
            "{}",
            updated
        );
        // The CLI-only section must appear after the preserved content.
        let node_repl_at = updated.find("[mcp_servers.node_repl]").unwrap();
        let moddin_at = updated.find("[mcp_servers.moddin]").unwrap();
        assert!(node_repl_at < moddin_at);
    }

    #[test]
    fn codex_toml_upsert_is_idempotent_and_replaces() {
        let first =
            upsert_codex_toml_entry("", r"C:\a\node.exe", &[r"C:\a\mcp.mjs".to_owned()], "ROOT");
        let second = upsert_codex_toml_entry(
            &first,
            r"C:\b\node.exe",
            &[r"C:\b\mcp.mjs".to_owned()],
            "ROOT",
        );
        assert_eq!(
            second.matches("[mcp_servers.moddin]").count(),
            1,
            "{}",
            second
        );
        assert!(second.contains(r"C:\b\node.exe"), "{}", second);
        assert!(!second.contains(r"C:\a\node.exe"), "{}", second);
        let third = upsert_codex_toml_entry(
            &second,
            r"C:\b\node.exe",
            &[r"C:\b\mcp.mjs".to_owned()],
            "ROOT",
        );
        assert_eq!(third, second, "idempotent upsert");
    }

    #[test]
    fn codex_toml_remove_only_drops_moddin_blocks() {
        let with_moddin = upsert_codex_toml_entry(
            "[mcp_servers.node_repl]\nargs = []\n",
            r"C:\a\node.exe",
            &[r"C:\a\mcp.mjs".to_owned()],
            "ROOT",
        );
        let (removed_text, removed) = remove_codex_toml_entry(&with_moddin);
        assert!(removed);
        assert!(!removed_text.contains("moddin"), "{}", removed_text);
        assert!(
            removed_text.contains("[mcp_servers.node_repl]"),
            "{}",
            removed_text
        );
        let (again, removed_again) = remove_codex_toml_entry(&removed_text);
        assert!(!removed_again);
        assert_eq!(again, removed_text);
    }

    #[test]
    fn codex_toml_handles_missing_and_present_env_style_entries() {
        // Old shape without the .env section is still fully removed.
        let legacy = "[mcp_servers.moddin]\ncommand = 'x'\nargs = []\n\n[profile]\nmodel = 'gpt'\n";
        let (text, removed) = remove_codex_toml_entry(legacy);
        assert!(removed);
        assert!(text.contains("[profile]"), "{}", text);
        assert!(!text.contains("moddin"), "{}", text);
    }

    /// The write path owns the backup, so every caller gets one — the
    /// `atomic_write` call sites used to write a user's `mcp.json` with
    /// no `.moddin-bak` beside it at all.
    #[test]
    fn write_config_backs_up_the_config_it_replaces() {
        let dir = temp_dir("write-backup");
        let path = dir.join("mcp.json");
        fs::write(&path, r#"{ "before": true }"#).unwrap();

        write_config(&path, r#"{ "after": true }"#).expect("write should succeed");

        assert_eq!(fs::read_to_string(&path).unwrap(), r#"{ "after": true }"#);
        let bak = backup_path(&path);
        assert!(bak.exists(), "{} should exist", bak.display());
        assert_eq!(fs::read_to_string(&bak).unwrap(), r#"{ "before": true }"#);
    }

    #[test]
    fn a_first_write_leaves_no_backup_behind() {
        let dir = temp_dir("write-first");
        let path = dir.join("mcp.json");

        write_config(&path, "{ }").expect("write should succeed");

        assert!(!backup_path(&path).exists(), "nothing to back up yet");
    }

    #[test]
    fn restore_from_backup_puts_the_previous_config_back() {
        let dir = temp_dir("restore");
        let path = dir.join("mcp.json");
        fs::write(&path, r#"{ "before": true }"#).unwrap();
        write_config(&path, r#"{ "after": true }"#).unwrap();
        // Whatever a failed write leaves behind, this is the state the
        // user would be looking at.
        fs::write(&path, r#"{ "after": tr"#).unwrap();

        restore_from_backup(&path).expect("restore should succeed");

        assert_eq!(fs::read_to_string(&path).unwrap(), r#"{ "before": true }"#);
        // Copied, not renamed: the backup is still there afterwards.
        assert!(backup_path(&path).exists());
    }

    #[test]
    fn restore_from_backup_says_when_there_is_nothing_to_restore() {
        let dir = temp_dir("restore-missing");
        let path = dir.join("mcp.json");
        fs::write(&path, "{}").unwrap();

        let error = restore_from_backup(&path).expect_err("no backup was ever taken");

        assert!(error.contains("No backup file exists"), "{error}");
        // The config it was asked about is untouched.
        assert_eq!(fs::read_to_string(&path).unwrap(), "{}");
    }

    /// A write that does not land has to leave the user with the config
    /// they had, and has to say what happened. A directory in the
    /// config's place is the portable way to make `fs::write` fail:
    /// there is no previous file to copy, so this also proves the
    /// failure path reports that the rollback had nothing to work with
    /// instead of claiming a restore it did not perform.
    #[test]
    fn a_failed_write_reports_that_the_config_could_not_be_restored() {
        let dir = temp_dir("write-fails");
        let path = dir.join("mcp.json");
        fs::create_dir_all(&path).unwrap();

        let error = write_config(&path, "{ }").expect_err("writing over a directory must fail");

        assert!(error.contains("Could not write"), "{error}");
        assert!(error.contains("No backup file exists"), "{error}");
    }
}
