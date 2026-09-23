//! Sequenced installer for "mod collections" — runs the capabilities
//! listed in a `CollectionSpec` one by one, in order, with a paused
//! state when a required step fails so the UI can ask the user how
//! to proceed.

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Mutex, OnceLock},
};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    capability::{CapabilitySpec, ResolvedConfig},
    capability_runner::{self, CapabilityRegistry, InstallResult},
    collection::{CollectionOrigin, CollectionRegistry, CollectionSpec},
};

/// In-memory session store. Sessions are short-lived: a `collection_install`
/// creates one, the user's "continue / abort" decision tears it down.
static SESSIONS: OnceLock<Mutex<HashMap<String, CollectionSession>>> = OnceLock::new();

fn sessions() -> &'static Mutex<HashMap<String, CollectionSession>> {
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Status of a single capability inside a session.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum StepStatus {
    Pending,
    Running,
    Completed,
    Skipped,
    Failed,
}

/// One row in the session ledger — the historical trace of what
/// happened to each capability the collection tried to install.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletedStep {
    pub capability_id: String,
    pub status: StepStatus,
    pub error: Option<String>,
    pub transaction_id: Option<String>,
}

/// Where the session is right now.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SessionStatus {
    /// Steps are still being run, or waiting for the first user call.
    Running,
    /// A required step failed. The frontend must call
    /// `collection_resume` with a decision.
    Paused,
    /// User chose to keep going after a required failure.
    Continuing,
    /// All steps ran to completion.
    Completed,
    /// User aborted — already-installed steps are being rolled back.
    Aborting,
    /// Session finished (success, aborted, or fully reverted).
    Closed,
}

/// The state machine. Lives in the in-memory `SESSIONS` map keyed by
/// a UUID the frontend echoes back.
#[derive(Debug, Clone)]
pub struct CollectionSession {
    pub id: String,
    pub collection_id: String,
    pub game_id: String,
    pub game_name: String,
    pub install_dir: PathBuf,
    pub executable_dir: PathBuf,
    pub config: ResolvedConfig,
    pub total_steps: usize,
    pub next_index: usize,
    pub steps: Vec<CompletedStep>,
    pub status: SessionStatus,
    /// When `status` is `Paused`, this captures why. The frontend
    /// shows it in the "continue or revert?" dialog.
    pub last_error: Option<String>,
    /// When `status` is `Paused`, the id of the capability that failed.
    pub failed_capability: Option<String>,
}

impl CollectionSession {
    fn snapshot(&self) -> CollectionSessionView {
        CollectionSessionView {
            id: self.id.clone(),
            collection_id: self.collection_id.clone(),
            game_id: self.game_id.clone(),
            game_name: self.game_name.clone(),
            status: self.status,
            total_steps: self.total_steps,
            next_index: self.next_index,
            steps: self.steps.clone(),
            last_error: self.last_error.clone(),
            failed_capability: self.failed_capability.clone(),
        }
    }
}

/// Read-only view the frontend consumes.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionSessionView {
    pub id: String,
    pub collection_id: String,
    pub game_id: String,
    pub game_name: String,
    pub status: SessionStatus,
    pub total_steps: usize,
    pub next_index: usize,
    pub steps: Vec<CompletedStep>,
    pub last_error: Option<String>,
    pub failed_capability: Option<String>,
}

/// Install request — kicks off a session and runs steps until the
/// first pause trigger.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionInstallRequest {
    pub collection_id: String,
    pub game_id: String,
    pub game_name: String,
    pub install_dir: String,
    pub executable_dir: String,
    #[serde(default)]
    pub config: ResolvedConfig,
}

/// Result of an install attempt. When `paused: true`, the frontend
/// must ask the user and call `collection_resume`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionInstallResult {
    pub ok: bool,
    pub paused: bool,
    pub session: CollectionSessionView,
    pub error: Option<String>,
}

/// User decision after a `Paused` state.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ResumeDecision {
    /// Keep going — the failed capability stays failed, the next
    /// required capability can still abort.
    Continue,
    /// Abort: roll back the already-installed steps in reverse order.
    Abort,
}

/// Result of resuming a session. Same shape as `CollectionInstallResult`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionResumeResult {
    pub ok: bool,
    pub paused: bool,
    pub session: CollectionSessionView,
    pub error: Option<String>,
}

/// Result of an explicit abort.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionAbortResult {
    pub ok: bool,
    pub session: CollectionSessionView,
    pub rolled_back: Vec<String>,
    pub errors: Vec<String>,
}

// ---------------------------------------------------------------------------
// Registry helpers
// ---------------------------------------------------------------------------

fn load_registry() -> CollectionRegistry {
    CollectionRegistry::load(crate::capability_runner::local_capabilities_dir().as_deref())
}

fn load_capability_registry() -> CapabilityRegistry {
    CapabilityRegistry::load()
}

fn reload_local_after_save() {
    // When a local collection YAML is saved, we have to re-scan the
    // local directory so the next `collection_list` sees it. We
    // piggy-back on the existing capability reload mechanism by
    // rebuilding both registries from disk.
    if let Some(dir) = crate::capability_runner::local_capabilities_dir() {
        // Capability registry has its own OnceLock with reload_local;
        // mirror that for collections below.
        rebuild_collection_registry_from(&dir);
    }
}

fn rebuild_collection_registry_from(dir: &std::path::Path) {
    // The collection registry isn't behind a static yet (it is
    // reconstructed on every command call via `load_registry`).
    // Nothing to do here — the next call to `collection_list` will
    // re-read the directory.
    let _ = dir;
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// List every collection Moddin currently knows about. Mirrors
/// `capability_list`.
#[tauri::command]
pub fn collection_list() -> Vec<crate::collection::CollectionSummary> {
    load_registry().summaries()
}

/// Fetch the full YAML body of one collection. The UI uses it to
/// render the install dialog's "what's inside" view.
#[tauri::command]
pub fn collection_get(id: String) -> Result<CollectionSpec, String> {
    load_registry()
        .get(&id)
        .cloned()
        .ok_or_else(|| format!("Unknown collection id '{id}'."))
}

/// Start a collection install. Runs each capability in order. Stops
/// at the first required failure and returns `paused: true` so the
/// UI can ask the user.
#[tauri::command]
pub fn collection_install(
    request: CollectionInstallRequest,
) -> CollectionInstallResult {
    let spec = match load_registry().get(&request.collection_id) {
        Some(s) => s.clone(),
        None => {
            return CollectionInstallResult {
                ok: false,
                paused: false,
                session: CollectionSessionView {
                    id: String::new(),
                    collection_id: request.collection_id.clone(),
                    game_id: request.game_id,
                    game_name: request.game_name,
                    status: SessionStatus::Closed,
                    total_steps: 0,
                    next_index: 0,
                    steps: Vec::new(),
                    last_error: Some(format!(
                        "Unknown collection id '{}'.",
                        request.collection_id
                    )),
                    failed_capability: None,
                },
                error: Some(format!(
                    "Unknown collection id '{}'.",
                    request.collection_id
                )),
            };
        }
    };

    let session = CollectionSession {
        id: Uuid::new_v4().to_string(),
        collection_id: spec.id.clone(),
        game_id: request.game_id,
        game_name: request.game_name,
        install_dir: PathBuf::from(request.install_dir),
        executable_dir: PathBuf::from(request.executable_dir),
        config: request.config,
        total_steps: spec.capabilities.len(),
        next_index: 0,
        steps: spec
            .capabilities
            .iter()
            .map(|c| CompletedStep {
                capability_id: c.id.clone(),
                status: StepStatus::Pending,
                error: None,
                transaction_id: None,
            })
            .collect(),
        status: SessionStatus::Running,
        last_error: None,
        failed_capability: None,
    };

    let id = session.id.clone();
    sessions()
        .lock()
        .expect("sessions poisoned")
        .insert(id.clone(), session);

    run_until_pause(&id)
}

/// Resume a paused session. `decision` is the user's choice.
#[tauri::command]
pub fn collection_resume(
    session_id: String,
    decision: ResumeDecision,
) -> CollectionResumeResult {
    {
        let mut guard = sessions().lock().expect("sessions poisoned");
        let Some(session) = guard.get_mut(&session_id) else {
            return CollectionResumeResult {
                ok: false,
                paused: false,
                session: CollectionSessionView {
                    id: session_id,
                    collection_id: String::new(),
                    game_id: String::new(),
                    game_name: String::new(),
                    status: SessionStatus::Closed,
                    total_steps: 0,
                    next_index: 0,
                    steps: Vec::new(),
                    last_error: Some("Session not found (already closed or never existed).".to_owned()),
                    failed_capability: None,
                },
                error: Some("Session not found.".to_owned()),
            };
        };

        match decision {
            ResumeDecision::Abort => {
                session.status = SessionStatus::Aborting;
            }
            ResumeDecision::Continue => {
                if let Some(failed_id) = session.failed_capability.clone() {
                    if let Some(step) =
                        session.steps.iter_mut().find(|s| s.capability_id == failed_id)
                    {
                        step.status = StepStatus::Failed;
                    }
                }
                session.status = SessionStatus::Continuing;
                session.failed_capability = None;
                session.last_error = None;
            }
        }
    }

    let result = run_until_pause(&session_id);
    CollectionResumeResult {
        ok: result.ok,
        paused: result.paused,
        session: result.session,
        error: result.error,
    }
}

/// Abort a session — uninstall everything that already installed,
/// in reverse order.
#[tauri::command]
pub fn collection_abort(session_id: String) -> CollectionAbortResult {
    {
        let mut guard = sessions().lock().expect("sessions poisoned");
        if let Some(session) = guard.get_mut(&session_id) {
            session.status = SessionStatus::Aborting;
        } else {
            return CollectionAbortResult {
                ok: false,
                session: CollectionSessionView {
                    id: session_id,
                    collection_id: String::new(),
                    game_id: String::new(),
                    game_name: String::new(),
                    status: SessionStatus::Closed,
                    total_steps: 0,
                    next_index: 0,
                    steps: Vec::new(),
                    last_error: Some("Session not found.".to_owned()),
                    failed_capability: None,
                },
                rolled_back: Vec::new(),
                errors: vec!["Session not found.".to_owned()],
            };
        }
    }

    let snapshot_before = snapshot_session(&session_id);
    let rolled_back = rollback_completed(&snapshot_before);
    let errors: Vec<String> = rolled_back
        .iter()
        .filter_map(|entry| match entry {
            RollbackEntry::RolledBack { .. } => None,
            RollbackEntry::Failed { error, .. } => Some(error.clone()),
        })
        .collect();
    let ok = errors.is_empty();
    let rolled_ids: Vec<String> = rolled_back
        .iter()
        .map(|entry| match entry {
            RollbackEntry::RolledBack { id } => id.clone(),
            RollbackEntry::Failed { id, .. } => id.clone(),
        })
        .collect();

    {
        let mut guard = sessions().lock().expect("sessions poisoned");
        if let Some(session) = guard.get_mut(&session_id) {
            session.status = SessionStatus::Closed;
            let view = session.snapshot();
            return CollectionAbortResult {
                ok,
                session: view,
                rolled_back: rolled_ids,
                errors,
            };
        }
    }
    CollectionAbortResult {
        ok,
        session: snapshot_before,
        rolled_back: rolled_ids,
        errors,
    }
}

fn snapshot_session(session_id: &str) -> CollectionSessionView {
    sessions()
        .lock()
        .expect("sessions poisoned")
        .get(session_id)
        .map(|s| s.snapshot())
        .unwrap_or_else(|| CollectionSessionView {
            id: session_id.to_owned(),
            collection_id: String::new(),
            game_id: String::new(),
            game_name: String::new(),
            status: SessionStatus::Closed,
            total_steps: 0,
            next_index: 0,
            steps: Vec::new(),
            last_error: Some("Session not found.".to_owned()),
            failed_capability: None,
        })
}

/// Validate a candidate YAML against the collection schema. Mirrors
/// `validate_capability_yaml` for symmetry with the AI authoring flow.
#[tauri::command]
pub fn collection_validate_yaml(yaml: String) -> CollectionValidationResult {
    let raw: serde_json::Value = match serde_yaml::from_str(&yaml) {
        Ok(v) => v,
        Err(error) => {
            return CollectionValidationResult {
                ok: false,
                errors: vec![format!("YAML parse error: {error}")],
                spec: None,
            };
        }
    };
    let mut errors = Vec::new();
    validate_collection_root(&raw, &mut errors);
    let spec: CollectionSpec = match serde_json::from_value(raw) {
        Ok(s) => s,
        Err(error) => {
            errors.push(format!("(root) {error}"));
            return CollectionValidationResult {
                ok: false,
                errors,
                spec: None,
            };
        }
    };
    validate_collection_spec(&spec, &mut errors);
    if !errors.is_empty() {
        return CollectionValidationResult {
            ok: false,
            errors,
            spec: None,
        };
    }
    CollectionValidationResult {
        ok: true,
        errors: Vec::new(),
        spec: Some(CollectionSpecSummary::from(&spec)),
    }
}

/// Save a user-authored collection to the local directory. Mirrors
/// `save_capability_yaml`. Reloads the collection registry so the new
/// file shows up on the next `collection_list`.
#[tauri::command]
pub fn collection_save_yaml(yaml: String, overwrite: bool) -> CollectionSaveResult {
    let validation = collection_validate_yaml(yaml.clone());
    if !validation.ok {
        return CollectionSaveResult {
            ok: false,
            id: String::new(),
            path: String::new(),
            overwrote: false,
            errors: validation.errors,
        };
    }
    let summary = match validation.spec {
        Some(s) => s,
        None => {
            return CollectionSaveResult {
                ok: false,
                id: String::new(),
                path: String::new(),
                overwrote: false,
                errors: vec!["Internal: collection summary missing after validation.".to_owned()],
            };
        }
    };

    let dir = match crate::capability_runner::local_capabilities_dir() {
        Some(d) => d.parent().map(|p| p.join("collections")),
        None => None,
    };
    let dir = match dir {
        Some(d) => d,
        None => {
            return CollectionSaveResult {
                ok: false,
                id: summary.id.clone(),
                path: String::new(),
                overwrote: false,
                errors: vec!["LOCALAPPDATA is not set; cannot save local collections.".to_owned()],
            };
        }
    };

    let target = dir.join(format!("{}.yaml", summary.id));
    let target_string = target.to_string_lossy().into_owned();
    let existed = target.exists();
    if existed && !overwrite {
        return CollectionSaveResult {
            ok: false,
            id: summary.id.clone(),
            path: target_string,
            overwrote: false,
            errors: vec![format!(
                "A local collection named '{}' already exists. Set overwrite=true to replace it.",
                summary.id
            )],
        };
    }

    if let Err(error) = std::fs::create_dir_all(&dir) {
        return CollectionSaveResult {
            ok: false,
            id: summary.id.clone(),
            path: target_string,
            overwrote: false,
            errors: vec![format!("Could not create {}: {error}", dir.display())],
        };
    }
    if let Err(error) = std::fs::write(&target, &yaml) {
        return CollectionSaveResult {
            ok: false,
            id: summary.id.clone(),
            path: target_string,
            overwrote: false,
            errors: vec![format!("Could not write {}: {error}", target.display())],
        };
    }

    reload_local_after_save();

    CollectionSaveResult {
        ok: true,
        id: summary.id,
        path: target_string,
        overwrote: existed,
        errors: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Result types for validate / save
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionValidationResult {
    pub ok: bool,
    pub errors: Vec<String>,
    pub spec: Option<CollectionSpecSummary>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionSpecSummary {
    pub id: String,
    pub display_name: String,
    pub category: String,
    pub status: String,
    pub target_game: Option<String>,
    pub capability_count: usize,
    pub required_count: usize,
}

impl From<&CollectionSpec> for CollectionSpecSummary {
    fn from(spec: &CollectionSpec) -> Self {
        Self {
            id: spec.id.clone(),
            display_name: spec.display_name.clone(),
            category: spec.category.clone(),
            status: spec.status.clone(),
            target_game: spec.target_game.clone(),
            capability_count: spec.capabilities.len(),
            required_count: spec.capabilities.iter().filter(|c| c.required).count(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionSaveResult {
    pub ok: bool,
    pub id: String,
    pub path: String,
    pub overwrote: bool,
    pub errors: Vec<String>,
}

// ---------------------------------------------------------------------------
// Schema validation
// ---------------------------------------------------------------------------

fn validate_collection_root(value: &serde_json::Value, errors: &mut Vec<String>) {
    let obj = match value.as_object() {
        Some(o) => o,
        None => {
            errors.push("(root) must be a mapping".to_owned());
            return;
        }
    };
    let allowed: &[&str] = &[
        "id",
        "displayName",
        "description",
        "category",
        "status",
        "targetGame",
        "requiredEngines",
        "capabilities",
        "safetyNotes",
    ];
    for key in obj.keys() {
        if !allowed.contains(&key.as_str()) {
            errors.push(format!("(root) unknown field '{key}'"));
        }
    }
    for required in ["id", "displayName", "category", "status", "capabilities"] {
        if !obj.contains_key(required) {
            errors.push(format!("(root) missing required field '{required}'"));
        }
    }
}

fn validate_collection_spec(spec: &CollectionSpec, errors: &mut Vec<String>) {
    let mut chars = spec.id.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => errors.push(".id must start with a lowercase ASCII letter".to_owned()),
    }
    if spec.id.chars().count() > 64 {
        errors.push(".id must be at most 64 characters".to_owned());
    }
    if spec.id.chars().any(|c| !c.is_ascii_lowercase() && !c.is_ascii_digit() && c != '-') {
        errors.push(".id may only contain lowercase letters, digits and dashes".to_owned());
    }
    if spec.display_name.trim().is_empty() {
        errors.push(".displayName must be a non-empty string".to_owned());
    }
    if !crate::collection::VALID_CATEGORIES.contains(&spec.category.as_str()) {
        errors.push(format!(
            ".category must be one of {:?}; got '{}'",
            crate::collection::VALID_CATEGORIES,
            spec.category
        ));
    }
    if !crate::collection::VALID_STATUSES.contains(&spec.status.as_str()) {
        errors.push(format!(
            ".status must be one of {:?}; got '{}'",
            crate::collection::VALID_STATUSES,
            spec.status
        ));
    }
    if spec.capabilities.is_empty() {
        errors.push(".capabilities must list at least one entry".to_owned());
    }
    let mut seen_ids = std::collections::HashSet::new();
    for (index, entry) in spec.capabilities.iter().enumerate() {
        if entry.id.trim().is_empty() {
            errors.push(format!(".capabilities[{index}].id must not be empty"));
            continue;
        }
        if !seen_ids.insert(entry.id.clone()) {
            errors.push(format!(
                ".capabilities[{index}].id duplicate entry '{}'",
                entry.id
            ));
        }
    }
}

// ---------------------------------------------------------------------------
// Step execution
// ---------------------------------------------------------------------------

/// Run pending steps until a required one fails (return Paused) or
/// every step is done (return Completed).
fn run_until_pause(session_id: &str) -> CollectionInstallResult {
    loop {
        let snapshot = {
            let mut guard = sessions().lock().expect("sessions poisoned");
            let Some(session) = guard.get_mut(session_id) else {
                return CollectionInstallResult {
                    ok: false,
                    paused: false,
                    session: CollectionSessionView {
                        id: session_id.to_owned(),
                        collection_id: String::new(),
                        game_id: String::new(),
                        game_name: String::new(),
                        status: SessionStatus::Closed,
                        total_steps: 0,
                        next_index: 0,
                        steps: Vec::new(),
                        last_error: Some("Session vanished mid-run.".to_owned()),
                        failed_capability: None,
                    },
                    error: Some("Session vanished mid-run.".to_owned()),
                };
            };

            // If user picked Abort, hand off to the rollback path.
            if session.status == SessionStatus::Aborting {
                let snap = session.snapshot();
                drop(guard);
                let rolled = rollback_completed(&snap);
                let mut guard = sessions().lock().expect("sessions poisoned");
                if let Some(s) = guard.get_mut(session_id) {
                    s.status = SessionStatus::Closed;
                }
                return CollectionInstallResult {
                    ok: rolled.iter().all(|r| matches!(r, RollbackEntry::RolledBack { .. })),
                    paused: false,
                    session: snap,
                    error: None,
                };
            }

            // Find the next pending step.
            let next_index = match session
                .steps
                .iter()
                .position(|s| s.status == StepStatus::Pending)
            {
                Some(i) => i,
                None => {
                    session.status = SessionStatus::Completed;
                    return CollectionInstallResult {
                        ok: true,
                        paused: false,
                        session: session.snapshot(),
                        error: None,
                    };
                }
            };

            let step_id = session.steps[next_index].capability_id.clone();
            // Look up required flag from the original spec; we keep
            // it in the session steps via the spec ref but to avoid
            // a borrow dance we just hold the bool here.
            session.steps[next_index].status = StepStatus::Running;
            session.next_index = next_index;
            let snap = session.snapshot();
            (snap, step_id, next_index)
            // guard released here
        };

        let registry = load_capability_registry();
        let spec = match registry.get(&snapshot.1) {
            Some(s) => s.clone(),
            None => {
                mark_step_failed(session_id, &snapshot.1, "Capability not found in registry");
                let entry_required = lookup_required(session_id, &snapshot.1);
                if entry_required {
                    pause(session_id, &snapshot.1, "Capability not found in registry");
                    return snapshot_as_paused(session_id);
                }
                continue;
            }
        };

        // We intentionally install capabilities without per-capability
        // config (the collection dialog asks for collection-level config
        // only — extending to per-capability overrides is reserved for a
        // future iteration).
        let config = {
            let guard = sessions().lock().expect("sessions poisoned");
            guard.get(session_id).map(|s| s.config.clone()).unwrap_or_default()
        };

        let result: Result<InstallResult, String> = capability_runner::run_install(
            &registry,
            &spec.id,
            &snapshot.0.game_id,
            &snapshot.0.game_name,
            &config,
            &snapshot_path(session_id, |s| s.install_dir.clone()),
            &snapshot_path(session_id, |s| s.executable_dir.clone()),
        );

        let entry_required = lookup_required(session_id, &snapshot.1);
        match result {
            Ok(install) => {
                mark_step_completed(session_id, &snapshot.1, install.transaction.map(|t| t.id));
            }
            Err(error) => {
                mark_step_failed(session_id, &snapshot.1, &error);
                if entry_required {
                    pause(session_id, &snapshot.1, &error);
                    return snapshot_as_paused(session_id);
                }
                // Non-required: skip and keep going.
                mark_step_skipped(session_id, &snapshot.1);
            }
        }

        let mut guard = sessions().lock().expect("sessions poisoned");
        if let Some(s) = guard.get_mut(session_id) {
            if s.steps.iter().all(|st| st.status != StepStatus::Pending) {
                s.status = SessionStatus::Completed;
                let view = s.snapshot();
                return CollectionInstallResult {
                    ok: true,
                    paused: false,
                    session: view,
                    error: None,
                };
            }
        }
    }
}

// Helper accessors below are tiny shims so we can keep `run_until_pause`
// readable. They snapshot under the lock and release before any await.

fn snapshot_path<F: Fn(&CollectionSession) -> PathBuf>(
    session_id: &str,
    pick: F,
) -> PathBuf {
    sessions()
        .lock()
        .expect("sessions poisoned")
        .get(session_id)
        .map(pick)
        .unwrap_or_default()
}

fn mark_step_completed(session_id: &str, capability_id: &str, transaction_id: Option<String>) {
    let mut guard = sessions().lock().expect("sessions poisoned");
    if let Some(session) = guard.get_mut(session_id) {
        if let Some(step) = session
            .steps
            .iter_mut()
            .find(|s| s.capability_id == capability_id)
        {
            step.status = StepStatus::Completed;
            step.transaction_id = transaction_id;
        }
    }
}

fn mark_step_failed(session_id: &str, capability_id: &str, error: &str) {
    let mut guard = sessions().lock().expect("sessions poisoned");
    if let Some(session) = guard.get_mut(session_id) {
        if let Some(step) = session
            .steps
            .iter_mut()
            .find(|s| s.capability_id == capability_id)
        {
            step.status = StepStatus::Failed;
            step.error = Some(error.to_owned());
        }
    }
}

fn mark_step_skipped(session_id: &str, capability_id: &str) {
    let mut guard = sessions().lock().expect("sessions poisoned");
    if let Some(session) = guard.get_mut(session_id) {
        if let Some(step) = session
            .steps
            .iter_mut()
            .find(|s| s.capability_id == capability_id)
        {
            step.status = StepStatus::Skipped;
        }
    }
}

fn lookup_required(session_id: &str, capability_id: &str) -> bool {
    sessions()
        .lock()
        .expect("sessions poisoned")
        .get(session_id)
        .and_then(|s| {
            let registry = load_registry();
            registry
                .get(&s.collection_id)
                .and_then(|c| c.capabilities.iter().find(|e| e.id == capability_id))
                .map(|e| e.required)
        })
        .unwrap_or(true)
}

fn pause(session_id: &str, capability_id: &str, error: &str) {
    let mut guard = sessions().lock().expect("sessions poisoned");
    if let Some(session) = guard.get_mut(session_id) {
        session.status = SessionStatus::Paused;
        session.failed_capability = Some(capability_id.to_owned());
        session.last_error = Some(error.to_owned());
    }
}

fn snapshot_as_paused(session_id: &str) -> CollectionInstallResult {
    let guard = sessions().lock().expect("sessions poisoned");
    match guard.get(session_id) {
        Some(session) => CollectionInstallResult {
            ok: false,
            paused: true,
            session: session.snapshot(),
            error: session.last_error.clone(),
        },
        None => CollectionInstallResult {
            ok: false,
            paused: false,
            session: CollectionSessionView {
                id: session_id.to_owned(),
                collection_id: String::new(),
                game_id: String::new(),
                game_name: String::new(),
                status: SessionStatus::Closed,
                total_steps: 0,
                next_index: 0,
                steps: Vec::new(),
                last_error: Some("Session not found.".to_owned()),
                failed_capability: None,
            },
            error: Some("Session not found.".to_owned()),
        },
    }
}

// ---------------------------------------------------------------------------
// Rollback (used by both paused-abort and explicit abort)
// ---------------------------------------------------------------------------

enum RollbackEntry {
    RolledBack { id: String },
    Failed { id: String, error: String },
}

fn rollback_completed(session: &CollectionSessionView) -> Vec<RollbackEntry> {
    let registry = load_capability_registry();
    let mut entries = Vec::new();

    // Reverse iteration: undo in opposite order.
    for step in session.steps.iter().rev() {
        if step.status != StepStatus::Completed {
            continue;
        }
        let id = step.capability_id.clone();
        match registry.get(&id) {
            Some(spec) => {
                let result = capability_runner::run_uninstall(
                    &registry,
                    spec.id.as_str(),
                    &session.game_id,
                    &session.game_name,
                    &session_install_dir(&session.id),
                );
                match result {
                    Ok(_) => entries.push(RollbackEntry::RolledBack { id }),
                    Err(error) => entries.push(RollbackEntry::Failed { id, error }),
                }
            }
            None => entries.push(RollbackEntry::Failed {
                id,
                error: "Capability not found in registry during rollback.".to_owned(),
            }),
        }
    }
    entries
}

fn session_install_dir(session_id: &str) -> PathBuf {
    sessions()
        .lock()
        .expect("sessions poisoned")
        .get(session_id)
        .map(|s| s.install_dir.clone())
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_yaml_minimal_collection() {
        let yaml = r#"
id: my-collection
displayName: My Collection
category: vr
status: available
capabilities:
  - id: optiscaler
    required: true
"#;
        let result = collection_validate_yaml(yaml.to_owned());
        assert!(result.ok, "errors: {:?}", result.errors);
        let summary = result.spec.expect("summary");
        assert_eq!(summary.id, "my-collection");
        assert_eq!(summary.capability_count, 1);
        assert_eq!(summary.required_count, 1);
    }

    #[test]
    fn validate_yaml_rejects_unknown_fields() {
        let yaml = r#"
id: my-collection
displayName: My Collection
category: vr
status: available
capabilities:
  - id: optiscaler
bogus: 42
"#;
        let result = collection_validate_yaml(yaml.to_owned());
        assert!(!result.ok);
        assert!(result.errors.iter().any(|e| e.contains("bogus")));
    }

    #[test]
    fn validate_yaml_rejects_empty_capabilities() {
        let yaml = r#"
id: my-collection
displayName: My Collection
category: vr
status: available
capabilities: []
"#;
        let result = collection_validate_yaml(yaml.to_owned());
        assert!(!result.ok);
        assert!(result
            .errors
            .iter()
            .any(|e| e.contains("at least one entry")));
    }

    #[test]
    fn validate_yaml_rejects_bad_category() {
        let yaml = r#"
id: my-collection
displayName: My Collection
category: shooter
status: available
capabilities:
  - id: optiscaler
"#;
        let result = collection_validate_yaml(yaml.to_owned());
        assert!(!result.ok);
        assert!(result.errors.iter().any(|e| e.contains("category")));
    }

    #[test]
    fn validate_yaml_rejects_duplicate_capability_id() {
        let yaml = r#"
id: my-collection
displayName: My Collection
category: vr
status: available
capabilities:
  - id: optiscaler
  - id: optiscaler
"#;
        let result = collection_validate_yaml(yaml.to_owned());
        assert!(!result.ok);
        assert!(result.errors.iter().any(|e| e.contains("duplicate")));
    }

    #[test]
    fn collection_list_includes_built_ins() {
        let summaries = collection_list();
        assert!(summaries.iter().any(|s| s.id == "vr-cyberpunk-essential"));
        assert!(summaries.iter().any(|s| s.id == "vr-elden-ring-starter"));
        for s in &summaries {
            assert_eq!(s.origin, CollectionOrigin::BuiltIn);
        }
    }

    #[test]
    fn collection_get_returns_built_in() {
        let spec = collection_get("vr-cyberpunk-essential".to_owned()).expect("present");
        assert_eq!(spec.id, "vr-cyberpunk-essential");
        assert_eq!(spec.target_game.as_deref(), Some("cyberpunk-2077"));
        assert!(spec.capabilities.len() >= 3);
    }

    #[test]
    fn collection_get_unknown_id_errors() {
        let result = collection_get("nope".to_owned());
        assert!(result.is_err());
    }
}
