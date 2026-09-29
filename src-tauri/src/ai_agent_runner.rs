//! Headless "agent mode": the app drives an AI CLI the user already has
//! installed (Codex, Claude Code or Cursor agent) so authoring a
//! capability never involves copy-pasting. The prompt asks the agent to
//! use the Moddin MCP tools (available when the client reads the config
//! `setup_ai_assistant` maintains) and to finish with the final
//! capability YAML in a fenced block. The app then routes the result
//! through the same validate → preview → save flow as the paste path —
//! nothing is saved without the user confirming the preview.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentKind {
    #[serde(rename = "codex")]
    Codex,
    #[serde(rename = "claudeCode")]
    ClaudeCode,
    #[serde(rename = "cursorAgent")]
    CursorAgent,
}

impl AgentKind {
    pub fn all() -> [AgentKind; 3] {
        [AgentKind::Codex, AgentKind::ClaudeCode, AgentKind::CursorAgent]
    }

    pub fn id(self) -> &'static str {
        match self {
            AgentKind::Codex => "codex",
            AgentKind::ClaudeCode => "claudeCode",
            AgentKind::CursorAgent => "cursorAgent",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            AgentKind::Codex => "Codex",
            AgentKind::ClaudeCode => "Claude Code",
            AgentKind::CursorAgent => "Cursor Agent",
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCliInfo {
    pub agent: AgentKind,
    pub available: bool,
    pub path: Option<String>,
    pub detail: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRunRequest {
    pub agent: AgentKind,
    pub prompt: String,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
}

fn default_timeout_secs() -> u64 {
    240
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentRunStatus {
    /// The agent replied with a parseable YAML fence.
    Ok,
    /// The run finished but no YAML fence was found; `output` carries the
    /// agent's final message so the user can read it (diagnose mode) or
    /// retry.
    NoYaml,
    /// The CLI failed, timed out or was cancelled.
    Failed,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRunResult {
    pub agent: AgentKind,
    pub status: AgentRunStatus,
    pub output: String,
    pub yaml: Option<String>,
    pub duration_ms: u64,
}

// -----------------------------------------------------------------------------
// CLI discovery
// -----------------------------------------------------------------------------

fn find_on_path(exe_name: &str) -> Option<PathBuf> {
    let output = std::process::Command::new("where.exe")
        .arg(exe_name)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && Path::new(line).is_file())
        .map(PathBuf::from)
}

/// The OpenAI Codex CLI ships with the Codex desktop app under
/// `%LOCALAPPDATA%\OpenAI\Codex\bin\<build-hash>\codex.exe`; the hash
/// changes every update, so glob the newest build and fall back to PATH.
fn find_codex_cli() -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)?
        .join("OpenAI")
        .join("Codex")
        .join("bin");
    let newest = std::fs::read_dir(&base).ok().into_iter().flatten().flatten()
        .map(|entry| entry.path().join("codex.exe"))
        .filter(|path| path.is_file())
        .max_by_key(|path| path.metadata().and_then(|m| m.modified()).ok());
    newest.or_else(|| find_on_path("codex.exe"))
}

fn find_claude_cli() -> Option<PathBuf> {
    find_on_path("claude.exe")
        .or_else(|| find_on_path("claude"))
        .or_else(|| {
            std::env::var_os("USERPROFILE")
                .map(PathBuf::from)
                .map(|home| home.join(".claude").join("bin").join("claude.exe"))
                .filter(|path| path.is_file())
        })
}

fn find_cursor_agent_cli() -> Option<PathBuf> {
    find_on_path("cursor-agent.exe")
        .or_else(|| find_on_path("cursor-agent"))
        .or_else(|| {
            std::env::var_os("USERPROFILE")
                .map(PathBuf::from)
                .map(|home| home.join(".cursor").join("bin").join("cursor-agent.exe"))
                .filter(|path| path.is_file())
        })
}

pub fn resolve_cli(agent: AgentKind) -> Option<PathBuf> {
    match agent {
        AgentKind::Codex => find_codex_cli(),
        AgentKind::ClaudeCode => find_claude_cli(),
        AgentKind::CursorAgent => find_cursor_agent_cli(),
    }
}

#[tauri::command]
pub fn list_agent_clis() -> Vec<AgentCliInfo> {
    AgentKind::all()
        .into_iter()
        .map(|agent| match resolve_cli(agent) {
            Some(path) => AgentCliInfo {
                agent,
                available: true,
                path: Some(path.display().to_string()),
                detail: None,
            },
            None => AgentCliInfo {
                agent,
                available: false,
                path: None,
                detail: Some(format!(
                    "{} CLI is not installed on this PC.",
                    agent.display_name()
                )),
            },
        })
        .collect()
}

// -----------------------------------------------------------------------------
// Argument vectors
// -----------------------------------------------------------------------------

/// Extra Codex flags. `--approve-for-me` routes approval requests
/// (including MCP tool calls) through automatic review while keeping a
/// sandbox; it cannot be combined with an explicit `--sandbox` flag. The
/// run is also `--ephemeral` and scoped to a scratch directory, so a
/// stuck approval fails loudly instead of hanging on an invisible
/// prompt.
///
/// These belong to the `exec` subcommand, not the bare `codex` command.
/// Current Codex (0.158.x) only accepts them after `exec`; passing them
/// to the top level fails with "unexpected argument
/// '--skip-git-repo-check' found", and the bare command would launch the
/// interactive TUI anyway.
#[cfg(target_os = "windows")]
const CODEX_EXTRA_ARGS: &[&str] = &[
    "--skip-git-repo-check",
    "--ephemeral",
    "--color",
    "never",
    "--approve-for-me",
];

/// Build the argv for one headless run. Extracted from the spawn call so
/// the per-agent flag shapes are unit-testable.
fn build_argv(
    agent: AgentKind,
    prompt: &str,
    scratch: &Path,
    last_message: &Path,
) -> Vec<String> {
    match agent {
        AgentKind::Codex => {
            // `exec` first: it is the documented non-interactive entry
            // point and the only place these flags are defined.
            let mut argv: Vec<String> = vec!["exec".to_owned()];
            argv.extend(CODEX_EXTRA_ARGS.iter().map(|s| s.to_string()));
            argv.push("-C".to_owned());
            argv.push(scratch.display().to_string());
            argv.push("--output-last-message".to_owned());
            argv.push(last_message.display().to_string());
            argv.push(prompt.to_owned());
            argv
        }
        AgentKind::ClaudeCode => vec![
            "-p".to_owned(),
            prompt.to_owned(),
            "--output-format".to_owned(),
            "text".to_owned(),
            "--max-turns".to_owned(),
            "12".to_owned(),
            "--dangerously-skip-permissions".to_owned(),
        ],
        AgentKind::CursorAgent => vec![
            "-p".to_owned(),
            "--force".to_owned(),
            "--output-format".to_owned(),
            "text".to_owned(),
            prompt.to_owned(),
        ],
    }
}

// -----------------------------------------------------------------------------
// Output handling
// -----------------------------------------------------------------------------

/// Strip ANSI escape sequences (SGR color codes and OSC titles) so the
/// final message is plain text.
fn strip_ansi(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\x1b' {
            out.push(c);
            continue;
        }
        match chars.peek().copied() {
            Some('[') => {
                // CSI: consume the '[' and everything up to the final
                // byte (a letter, or '~').
                chars.next();
                for c in chars.by_ref() {
                    if c.is_ascii_alphabetic() || c == '~' {
                        break;
                    }
                }
            }
            Some(']') => {
                // OSC: consume the ']' and everything up to BEL or ESC \.
                chars.next();
                let mut prev = '\0';
                for c in chars.by_ref() {
                    if c == '\x07' || (prev == '\x1b' && c == '\\') {
                        break;
                    }
                    prev = c;
                }
            }
            _ => {}
        }
    }
    out
}

/// Extract the LAST fenced ```yaml block. Agents often print intermediate
/// drafts; the final recipe is the last fence. Returns the fenced body
/// trimmed, or None when there is no yaml fence at all.
fn extract_yaml_fence(input: &str) -> Option<String> {
    let mut last: Option<&str> = None;
    let mut rest = input;
    while let Some(start) = rest.find("```") {
        let after_ticks = &rest[start + 3..];
        let lang_end = after_ticks
            .find(|c: char| c == '\n' || c == '\r')
            .unwrap_or(after_ticks.len());
        let lang = after_ticks[..lang_end].trim().to_ascii_lowercase();
        let body_start = start + 3 + lang_end;
        let search_from = &rest[body_start..];
        let body_end = search_from.find("```")?;
        if lang == "yaml" || lang == "yml" {
            last = Some(&search_from[..body_end]);
        }
        rest = &search_from[body_end + 3..];
    }
    last.map(str::trim).map(str::to_owned).filter(|s| !s.is_empty())
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_owned();
    }
    let tail: String = s.chars().skip(s.chars().count() - max).collect();
    format!("…{tail}")
}

// -----------------------------------------------------------------------------
// The run
// -----------------------------------------------------------------------------

/// Only one agent run at a time per app instance: two LLM runs racing
/// would burn quota and confuse the single dialog state.
static RUNNING: AtomicBool = AtomicBool::new(false);

fn scratch_dir(agent: AgentKind) -> Option<PathBuf> {
    let dir = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)?
        .join("Moddin")
        .join("agent-runs")
        .join(format!("{}-{}", agent.id(), std::process::id()));
    if std::fs::create_dir_all(&dir).is_ok() {
        Some(dir)
    } else {
        None
    }
}

#[tauri::command]
pub async fn run_ai_agent_prompt(request: AgentRunRequest) -> Result<AgentRunResult, String> {
    if request.prompt.trim().is_empty() {
        return Err("The prompt is empty.".to_owned());
    }
    if RUNNING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err("Another AI run is already in progress.".to_owned());
    }
    let result = run_inner(&request).await;
    RUNNING.store(false, Ordering::SeqCst);
    result
}

async fn run_inner(request: &AgentRunRequest) -> Result<AgentRunResult, String> {
    let started = Instant::now();
    let exe = resolve_cli(request.agent)
        .ok_or_else(|| format!("{} CLI was not found.", request.agent.display_name()))?;
    let scratch = scratch_dir(request.agent)
        .ok_or_else(|| "Could not create a scratch directory for the AI run.".to_owned())?;
    let last_message = scratch.join("last-message.txt");
    let _ = std::fs::remove_file(&last_message);

    let argv = build_argv(request.agent, &request.prompt, &scratch, &last_message);

    let mut command = std::process::Command::new(&exe);
    command
        .args(&argv)
        .current_dir(&scratch)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = command
        .spawn()
        .map_err(|error| format!("Could not start {}: {error}", exe.display()))?;

    // Poll-wait so the timeout actually kills a stuck CLI.
    let timeout = std::time::Duration::from_secs(request.timeout_secs.max(30));
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {
                if started.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            Err(error) => return Err(format!("Could not wait on {}: {error}", exe.display())),
        }
    };

    let mut stdout = String::new();
    let mut stderr = String::new();
    if let Some(mut out) = child.stdout.take() {
        use std::io::Read;
        let _ = out.read_to_string(&mut stdout);
    }
    if let Some(mut err) = child.stderr.take() {
        use std::io::Read;
        let _ = err.read_to_string(&mut stderr);
    }

    let duration_ms = started.elapsed().as_millis() as u64;
    let _ = std::fs::remove_dir_all(&scratch);

    let status = match status {
        Some(status) => status,
        None => {
            return Ok(AgentRunResult {
                agent: request.agent,
                status: AgentRunStatus::Failed,
                output: format!(
                    "{} did not finish within {} minutes.",
                    request.agent.display_name(),
                    request.timeout_secs.max(30) / 60
                ),
                yaml: None,
                duration_ms,
            });
        }
    };

    // Prefer the CLI's own "last message" capture; it is exactly the
    // final answer without progress noise. Fall back to stdout.
    let raw_output = std::fs::read_to_string(&last_message)
        .unwrap_or_else(|_| stdout.clone());
    let output = truncate_chars(&strip_ansi(&raw_output).trim().to_owned(), 20_000);

    if !status.success() {
        let detail = if stderr.trim().is_empty() {
            output.clone()
        } else {
            truncate_chars(&strip_ansi(&stderr).trim().to_owned(), 2_000)
        };
        return Ok(AgentRunResult {
            agent: request.agent,
            status: AgentRunStatus::Failed,
            output: format!(
                "{} exited with an error. {detail}",
                request.agent.display_name()
            ),
            yaml: None,
            duration_ms,
        });
    }

    let yaml = extract_yaml_fence(&raw_output);
    Ok(AgentRunResult {
        agent: request.agent,
        status: if yaml.is_some() {
            AgentRunStatus::Ok
        } else {
            AgentRunStatus::NoYaml
        },
        output,
        yaml,
        duration_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_the_last_yaml_fence() {
        let text = "draft:\n```yaml\nid: first\n```\nmore text\n```yaml\nid: final\ndisplayName: Final\n```\ntrailing";
        let yaml = extract_yaml_fence(text).expect("yaml found");
        assert!(yaml.starts_with("id: final"), "{}", yaml);
        assert!(yaml.contains("displayName: Final"));
    }

    #[test]
    fn ignores_non_yaml_fences() {
        let text = "```json\n{\"a\":1}\n```\n```yaml\nid: ok\n```";
        let yaml = extract_yaml_fence(text).expect("yaml found");
        assert_eq!(yaml, "id: ok");
    }

    #[test]
    fn accepts_yml_and_languageless_first_line() {
        assert_eq!(extract_yaml_fence("```yml\nid: a\n```").unwrap(), "id: a");
    }

    #[test]
    fn returns_none_without_fence() {
        assert!(extract_yaml_fence("just prose").is_none());
        assert!(extract_yaml_fence("```yaml\n\n```").is_none());
    }

    #[test]
    fn unclosed_fence_is_not_extracted() {
        assert!(extract_yaml_fence("```yaml\nid: never-closed").is_none());
    }

    #[test]
    fn strips_sgr_and_osc_sequences() {
        assert_eq!(strip_ansi("\x1b[32mgreen\x1b[0m plain"), "green plain");
        assert_eq!(strip_ansi("\x1b]0;title\x07text"), "text");
        assert_eq!(strip_ansi("no escapes"), "no escapes");
    }

    #[test]
    fn codex_argv_uses_scratch_last_message_and_prompt() {
        let scratch = Path::new("S");
        let last = Path::new("S/last.txt");
        let argv = build_argv(AgentKind::Codex, "PROMPT", scratch, last);
        let joined = argv.join(" ");
        assert!(joined.contains("--skip-git-repo-check"), "{}", joined);
        assert!(joined.contains("--ephemeral"), "{}", joined);
        assert!(joined.contains("--approve-for-me"), "{}", joined);
        assert!(!joined.contains("--sandbox"), "{}", joined);
        assert!(joined.contains("-C S"), "{}", joined);
        // Regression: these flags moved under the `exec` subcommand in
        // current Codex. Against the bare command it dies with
        // "unexpected argument '--skip-git-repo-check' found" and the
        // user sees the raw CLI error in the AI panel.
        assert_eq!(argv.first().map(String::as_str), Some("exec"), "{}", joined);
        let exec_index = argv.iter().position(|arg| arg == "exec").expect("exec");
        for flag in ["--skip-git-repo-check", "--ephemeral", "--color", "--approve-for-me"] {
            let index = argv
                .iter()
                .position(|arg| arg == flag)
                .unwrap_or_else(|| panic!("{flag} missing from {joined}"));
            assert!(
                index > exec_index,
                "{flag} must come after `exec`, not before it: {joined}"
            );
        }        assert!(joined.contains("--output-last-message S/last.txt"), "{}", joined);
        assert!(argv.last().is_some_and(|a| a == "PROMPT"));
    }

    #[test]
    fn claude_and_cursor_argv_carry_the_prompt() {
        let argv = build_argv(AgentKind::ClaudeCode, "PROMPT", Path::new("S"), Path::new("L"));
        assert!(argv.contains(&"-p".to_owned()));
        assert!(argv.contains(&"PROMPT".to_owned()));
        assert!(argv.contains(&"--dangerously-skip-permissions".to_owned()));

        let argv = build_argv(AgentKind::CursorAgent, "PROMPT", Path::new("S"), Path::new("L"));
        assert!(argv.contains(&"--force".to_owned()));
        assert!(argv.last().is_some_and(|a| a == "PROMPT"));
    }

    #[test]
    fn agent_ids_round_trip_through_serde() {
        #[derive(Debug, Serialize, Deserialize)]
        struct Wrapper {
            agent: AgentKind,
        }
        for agent in AgentKind::all() {
            let wrapped = Wrapper { agent };
            let json = serde_json::to_string(&wrapped).unwrap();
            let back: Wrapper = serde_json::from_str(&json).unwrap();
            assert_eq!(back.agent, agent);
            assert!(json.contains(agent.id()), "{}", json);
        }
    }

    #[test]
    fn truncate_shortens_from_the_left() {
        let long = "x".repeat(30_000);
        let cut = truncate_chars(&long, 20_000);
        assert_eq!(cut.chars().count(), 20_001);
        assert!(cut.starts_with('…'));
    }
}
