//! Backend for the "Ask AI to author a capability" workflow that lives
//! inside the Moddin UI.
//!
//! The user's flow:
//!   1. Click an "Ask AI" button somewhere in the UI.
//!   2. The frontend asks [`build_author_prompt`] for a self-contained
//!      prompt (game context + schema + example + the user's intent).
//!   3. The user copies the prompt into their AI of choice and pastes
//!      the YAML it returns back into the dialog.
//!   4. The frontend calls [`validate_capability_yaml`] (schema check)
//!      and [`preview_capability_plan`] (dry-run diff) so the user can
//!      see what the install would touch before anything runs.
//!   5. On explicit confirmation the frontend calls
//!      [`save_capability_yaml`], which writes the file under
//!      `%LOCALAPPDATA%/Moddin/capabilities/<id>.yaml` and reloads the
//!      registry so the new capability shows up in the UI immediately.
//!
//! Every command runs server-side, so the schema is the single source
//! of truth — same enums and patterns as the JS schema
//! (`moddin-agent/schema/capability.schema.json`).

use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    sync::{Mutex, OnceLock},
};

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::{
    capability::{CapabilitySpec, CheckSpec, StepSpec},
    capability_runner::{local_capabilities_dir, CapabilityRegistry},
};

/// Allow-list of `category` values, mirrored from the JSON schema.
const VALID_CATEGORIES: &[&str] = &["vr", "graphics", "qol", "system"];

/// Allow-list of `status` values, mirrored from the JSON schema.
const VALID_STATUSES: &[&str] = &["available", "planned"];

/// Allow-list of step kinds. Mirrors
/// `src-tauri/src/builtin_steps.rs::known_kinds()`.
const VALID_STEP_KINDS: &[&str] = &[
    "extract-zip",
    "verify-hash",
    "file-delete",
    "write-text-file",
    "write-binary-file",
    "move-file",
    "spawn-process",
    "kill-process",
    "registry-write",
    "registry-delete",
];

/// Allow-list of check kinds. Mirrors
/// `src-tauri/src/builtin_checks.rs::known_kinds()`.
const VALID_CHECK_KINDS: &[&str] = &[
    "process-running",
    "file-exists",
    "file-absent",
    "archive-reachable",
    "archive-sha256",
];

/// Allow-list of severity strings.
const VALID_SEVERITIES: &[&str] = &["info", "warning", "blocker"];

/// Allow-list of category strings for checks.
const VALID_CHECK_CATEGORIES: &[&str] = &["global", "category", "modulespecific"];

/// Allow-list of `configField.type` values.
const VALID_CONFIG_FIELD_TYPES: &[&str] = &[
    "string", "number", "boolean", "url", "sha256", "path", "enum",
];

/// Regex shape of a capability id (kebab-case, ASCII).
fn is_valid_capability_id(id: &str) -> bool {
    let mut chars = id.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    let len = id.chars().count();
    if !(2..=64).contains(&len) {
        return false;
    }
    id.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Regex shape of a `configField.name` (PascalCase / camelCase, ASCII).
fn is_valid_config_field_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Regex shape of a `check.id` (kebab-case, ASCII).
fn is_valid_check_id(id: &str) -> bool {
    let mut chars = id.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    if id.chars().count() < 2 {
        return false;
    }
    id.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

// ---------------------------------------------------------------------------
// Public command result types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorValidationResult {
    pub ok: bool,
    pub errors: Vec<String>,
    /// Parsed spec, echoed back so the UI can show a summary preview
    /// without re-parsing.
    pub spec: Option<AuthorSpecSummary>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorSpecSummary {
    pub id: String,
    pub display_name: String,
    pub category: String,
    pub status: String,
    pub supported_engines: Vec<String>,
    pub config_fields: Vec<String>,
    pub install_steps: usize,
    pub uninstall_steps: usize,
    pub verify_checks: usize,
    pub safety_notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorPreviewResult {
    pub ok: bool,
    pub plan: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorSaveResult {
    pub ok: bool,
    pub id: String,
    pub path: String,
    /// True when the saved file overrode an existing local capability.
    pub overwrote: bool,
    /// Errors that surfaced while validating before save. When this is
    /// non-empty the file was NOT written.
    pub errors: Vec<String>,
}

/// Single recommendation the AI returned. Mirrors the YAML shape:
/// `{ type, id, reason, confidence }`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Recommendation {
    /// `collection` (bundle of capabilities) or `capability` (single mod).
    #[serde(rename = "type")]
    pub kind: RecommendationKind,
    pub id: String,
    pub reason: String,
    pub confidence: f32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum RecommendationKind {
    Collection,
    Capability,
}

/// Result of validating the YAML the AI returned for a recommendation
/// request. `ok: true` carries the parsed list back to the UI.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecommendationsValidationResult {
    pub ok: bool,
    pub errors: Vec<String>,
    pub recommendations: Vec<Recommendation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorPromptContext {
    pub mode: AuthorPromptMode,
    /// `basic` walks the user through downloading an AI and writing a
    /// plain-language intent; `advanced` assumes the user already has
    /// an AI open and just needs a terse prompt + the schema.
    #[serde(default)]
    pub verbosity: AuthorPromptVerbosity,
    /// Game id the user picked in the UI (e.g. `elden-ring`).
    pub game_id: Option<String>,
    /// Human-readable game name, used in the prompt's English block.
    pub game_name: Option<String>,
    /// Capability id the user is iterating on. Used by the `improve`
    /// and `diagnose` modes.
    pub capability_id: Option<String>,
    /// Free-text description from the user.
    pub intent: Option<String>,
    /// Error message that triggered the diagnose mode. Used as the
    /// "what went wrong" block in the prompt.
    pub error_message: Option<String>,
    /// Where the error came from — game, capability, transaction…
    pub error_source: Option<String>,
    /// Snapshot of the capabilities the user can currently see
    /// (built-in + local + community). Used by the `recommend` mode
    /// to ground the AI in the actual catalog.
    #[serde(default)]
    pub capability_catalog: Vec<CatalogCapability>,
    /// Snapshot of the collections the user can currently see.
    /// Same role as `capability_catalog` for the collection side.
    #[serde(default)]
    pub collection_catalog: Vec<CatalogCollection>,
}

/// Lightweight capability snapshot for the recommend prompt. The AI
/// uses it to pick concrete recommendations; the UI uses the ids to
/// drive installs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogCapability {
    pub id: String,
    pub display_name: String,
    pub category: String,
    pub description: String,
    pub target_game: Option<String>,
}

/// Lightweight collection snapshot, same role as CatalogCapability.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogCollection {
    pub id: String,
    pub display_name: String,
    pub category: String,
    pub description: String,
    pub target_game: Option<String>,
    pub capability_count: usize,
}

impl Default for AuthorPromptContext {
    fn default() -> Self {
        Self {
            mode: AuthorPromptMode::Author,
            verbosity: AuthorPromptVerbosity::default(),
            game_id: None,
            game_name: None,
            capability_id: None,
            intent: None,
            error_message: None,
            error_source: None,
            capability_catalog: Vec::new(),
            collection_catalog: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum AuthorPromptMode {
    /// Author a brand-new capability from scratch.
    Author,
    /// Improve / modify an existing capability.
    Improve,
    /// Diagnose a Moddin error with AI help.
    Diagnose,
    /// Ask the AI to recommend existing capabilities / collections
    /// for what the user wants to do.
    Recommend,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum AuthorPromptVerbosity {
    /// Plain Portuguese prose with technical terms kept in parentheses
    /// so the AI still understands the schema. Default — the dialog
    /// opens here for non-developer users.
    #[default]
    Basic,
    /// Terse English prompt aimed at a developer or an AI agent that
    /// already knows what a YAML schema is.
    Advanced,
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// Generate the prompt text the user copies into their AI of choice.
/// Pure function of the request — no I/O — so the frontend can call it
/// without any side effects.
#[tauri::command]
pub fn build_author_prompt(context: AuthorPromptContext) -> String {
    build_prompt_text(&context)
}

/// Validate a YAML string against the Moddin capability schema. The
/// parser is the same one the runner uses for built-in / local
/// capabilities; on top of it we layer the JSON-schema constraints
/// (enums, patterns, `additionalProperties: false`).
#[tauri::command]
pub fn validate_recommendations_yaml(yaml: String) -> RecommendationsValidationResult {
    validate_recommendations_string(&yaml)
}

#[tauri::command]
pub fn validate_capability_yaml(yaml: String) -> AuthorValidationResult {
    validate_yaml_string(&yaml)
}

/// Static dry-run: read the parsed capability and return a human-readable
/// summary of what an install would touch. Does NOT execute anything.
#[tauri::command]
pub fn preview_capability_plan(yaml: String) -> AuthorPreviewResult {
    match validate_yaml_string(&yaml) {
        AuthorValidationResult {
            ok: false,
            errors,
            ..
        } => AuthorPreviewResult {
            ok: false,
            plan: format!(
                "Spec is not schema-valid; fix these errors first:\n{}",
                errors
                    .iter()
                    .map(|e| format!("  - {e}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            ),
        },
        AuthorValidationResult {
            spec: Some(summary),
            ..
        } => AuthorPreviewResult {
            ok: true,
            plan: render_preview(&summary, &yaml),
        },
        AuthorValidationResult { ok: true, .. } => AuthorPreviewResult {
            ok: true,
            plan: "Spec is valid.".to_owned(),
        },
    }
}

/// Persist a schema-valid capability YAML to the user's local Moddin
/// capabilities folder. On success the registry is reloaded so the new
/// capability shows up in the UI immediately. The user then opens the
/// game and clicks Apply as usual.
#[tauri::command]
pub fn save_capability_yaml(yaml: String, overwrite: bool) -> AuthorSaveResult {
    let validation = validate_yaml_string(&yaml);
    if !validation.ok {
        return AuthorSaveResult {
            ok: false,
            id: String::new(),
            path: String::new(),
            overwrote: false,
            errors: validation.errors,
        };
    }
    let spec = match validation.spec {
        Some(s) => s,
        None => {
            return AuthorSaveResult {
                ok: false,
                id: String::new(),
                path: String::new(),
                overwrote: false,
                errors: vec!["Internal: spec summary missing after validation.".to_owned()],
            };
        }
    };

    let dir = match local_capabilities_dir() {
        Some(d) => d,
        None => {
            return AuthorSaveResult {
                ok: false,
                id: spec.id,
                path: String::new(),
                overwrote: false,
                errors: vec![
                    "LOCALAPPDATA is not set on this platform. Set MODDIN_LOCAL_CAPABILITIES_DIR to an explicit directory before saving.".to_owned(),
                ],
            };
        }
    };

    let target = dir.join(format!("{}.yaml", spec.id));
    let target_string = target.to_string_lossy().into_owned();
    let existed = target.exists();
    if existed && !overwrite {
        return AuthorSaveResult {
            ok: false,
            id: spec.id.clone(),
            path: target_string,
            overwrote: false,
            errors: vec![format!(
                "A local capability named '{}' already exists. Set overwrite=true to replace it.",
                spec.id
            )],
        };
    }

    if let Err(error) = fs::create_dir_all(&dir) {
        return AuthorSaveResult {
            ok: false,
            id: spec.id.clone(),
            path: target_string,
            overwrote: false,
            errors: vec![format!("Could not create {}: {error}", dir.display())],
        };
    }
    if let Err(error) = fs::write(&target, &yaml) {
        return AuthorSaveResult {
            ok: false,
            id: spec.id.clone(),
            path: target_string,
            overwrote: false,
            errors: vec![format!("Could not write {}: {error}", target.display())],
        };
    }

    // Reload the in-memory registry so the next capability_list call
    // returns the new spec.
    reload_registry_from(&dir);

    AuthorSaveResult {
        ok: true,
        id: spec.id,
        path: target_string,
        overwrote: existed,
        errors: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Internal helpers (used by the commands and by tests)
// ---------------------------------------------------------------------------

/// Parse + schema-validate a recommendation YAML. The shape is much
/// simpler than a capability YAML — it's a read-only artifact the
/// frontend consumes to render a checklist. We still validate ids
/// land in the catalog the request carried (when one was provided)
/// so the UI doesn't show "install" buttons for hallucinated items.
fn validate_recommendations_string(yaml: &str) -> RecommendationsValidationResult {
    let raw: JsonValue = match serde_yaml::from_str(yaml) {
        Ok(value) => value,
        Err(error) => {
            return RecommendationsValidationResult {
                ok: false,
                errors: vec![format!("YAML parse error: {error}")],
                recommendations: Vec::new(),
            };
        }
    };

    let parsed: Vec<Recommendation> = match serde_json::from_value(raw) {
        Ok(list) => list,
        Err(error) => {
            return RecommendationsValidationResult {
                ok: false,
                errors: vec![format!("Recommendations list is malformed: {error}")],
                recommendations: Vec::new(),
            };
        }
    };

    let mut errors = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut sanitized = Vec::new();
    for (index, rec) in parsed.iter().enumerate() {
        if rec.id.trim().is_empty() {
            errors.push(format!("recommendations[{index}].id must not be empty"));
            continue;
        }
        if !seen.insert((rec.kind, rec.id.clone())) {
            errors.push(format!(
                "recommendations[{index}] duplicates {kind:?} '{id}'",
                kind = rec.kind,
                id = rec.id
            ));
            continue;
        }
        if !(0.0..=1.0).contains(&rec.confidence) {
            errors.push(format!(
                "recommendations[{index}].confidence must be between 0 and 1 (got {})",
                rec.confidence
            ));
            continue;
        }
        if rec.confidence < 0.4 {
            // Below the threshold the prompt asked the AI to use; drop
            // silently rather than fail validation.
            continue;
        }
        sanitized.push(rec.clone());
    }

    if !errors.is_empty() {
        return RecommendationsValidationResult {
            ok: false,
            errors,
            recommendations: Vec::new(),
        };
    }

    RecommendationsValidationResult {
        ok: true,
        errors: Vec::new(),
        recommendations: sanitized,
    }
}

/// Parse + schema-validate. Returns the spec summary on success so the
/// caller doesn't have to re-parse for the preview / save path.
fn validate_yaml_string(yaml: &str) -> AuthorValidationResult {
    let raw: JsonValue = match serde_yaml::from_str(yaml) {
        Ok(value) => value,
        Err(error) => {
            return AuthorValidationResult {
                ok: false,
                errors: vec![format!("YAML parse error: {error}")],
                spec: None,
            };
        }
    };

    let mut errors: Vec<String> = Vec::new();

    // Top-level shape (additionalProperties: false + required fields).
    validate_root(&raw, &mut errors);

    // Parse into the strongly typed spec so we can run the rest of the
    // schema rules (enums, patterns, additionalProperties on nested
    // objects) against the same fields the runner consumes.
    let spec: CapabilitySpec = match serde_json::from_value(raw.clone()) {
        Ok(s) => s,
        Err(error) => {
            errors.push(format!("(root) {error}"));
            return AuthorValidationResult {
                ok: false,
                errors,
                spec: None,
            };
        }
    };

    validate_capability_spec(&spec, &mut errors);

    if !errors.is_empty() {
        return AuthorValidationResult {
            ok: false,
            errors,
            spec: None,
        };
    }

    AuthorValidationResult {
        ok: true,
        errors: Vec::new(),
        spec: Some(summarize_spec(&spec)),
    }
}

fn validate_root(value: &JsonValue, errors: &mut Vec<String>) {
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
        "category",
        "status",
        "supportedEngines",
        "safetyNotes",
        "configSchema",
        "checks",
        "verify",
        "install",
        "uninstall",
    ];
    for key in obj.keys() {
        if !allowed.contains(&key.as_str()) {
            errors.push(format!("(root) unknown field '{key}'"));
        }
    }
    for required in ["id", "displayName", "category", "status"] {
        if !obj.contains_key(required) {
            errors.push(format!("(root) missing required field '{required}'"));
        }
    }
}

fn validate_capability_spec(spec: &CapabilitySpec, errors: &mut Vec<String>) {
    if !is_valid_capability_id(&spec.id) {
        errors.push(format!(
            ".id must match ^[a-z][a-z0-9-]{{1,63}}$; got '{}'",
            spec.id
        ));
    }
    if spec.display_name.trim().is_empty() {
        errors.push(".displayName must be a non-empty string".to_owned());
    }
    if !VALID_CATEGORIES.contains(&spec.category.as_str()) {
        errors.push(format!(
            ".category must be one of {VALID_CATEGORIES:?}; got '{}'",
            spec.category
        ));
    }
    if !VALID_STATUSES.contains(&spec.status.as_str()) {
        errors.push(format!(
            ".status must be one of {VALID_STATUSES:?}; got '{}'",
            spec.status
        ));
    }

    let mut seen_config_names = std::collections::HashSet::new();
    for (index, field) in spec.config_schema.iter().enumerate() {
        let path = format!(".configSchema[{index}]");
        if !is_valid_config_field_name(&field.name) {
            errors.push(format!(
                "{path}.name must match ^[A-Za-z][A-Za-z0-9_]*$; got '{}'",
                field.name
            ));
        }
        if !seen_config_names.insert(field.name.clone()) {
            errors.push(format!(
                "{path}.name duplicate config field '{}'",
                field.name
            ));
        }
        if !VALID_CONFIG_FIELD_TYPES.contains(&field.field_type.as_str()) {
            errors.push(format!(
                "{path}.type must be one of {VALID_CONFIG_FIELD_TYPES:?}; got '{}'",
                field.field_type
            ));
        }
        validate_object_keys(&field_params(&field.name, "params"), &[], errors, &path);
    }

    for (index, check) in spec.checks.iter().chain(spec.verify.iter()).enumerate() {
        validate_check(check, errors, index);
    }
    for (index, step) in spec.install.iter().chain(spec.uninstall.iter()).enumerate() {
        validate_step(step, errors, index);
    }
}

fn validate_check(check: &CheckSpec, errors: &mut Vec<String>, index: usize) {
    let path = format!(".checks[{index}]");
    if !is_valid_check_id(&check.id) {
        errors.push(format!(
            "{path}.id must match ^[a-z][a-z0-9-]+$; got '{}'",
            check.id
        ));
    }
    if check.label.trim().is_empty() {
        errors.push(format!("{path}.label must be a non-empty string"));
    }
    if !VALID_CHECK_KINDS.contains(&check.kind.as_str()) {
        errors.push(format!(
            "{path}.kind must be one of {VALID_CHECK_KINDS:?}; got '{}'",
            check.kind
        ));
    }
    if !check.severity.is_empty() && !VALID_SEVERITIES.contains(&check.severity.as_str()) {
        errors.push(format!(
            "{path}.severity must be one of {VALID_SEVERITIES:?}; got '{}'",
            check.severity
        ));
    }
    if !VALID_CHECK_CATEGORIES.contains(&check.category.as_str()) {
        errors.push(format!(
            "{path}.category must be one of {VALID_CHECK_CATEGORIES:?}; got '{}'",
            check.category
        ));
    }
}

fn validate_step(step: &StepSpec, errors: &mut Vec<String>, index: usize) {
    let path = format!(".install[{index}]");
    if !VALID_STEP_KINDS.contains(&step.kind.as_str()) {
        errors.push(format!(
            "{path}.kind must be one of {VALID_STEP_KINDS:?}; got '{}'",
            step.kind
        ));
    }
}

/// Workaround helper: the `params` map is typed as
/// `BTreeMap<String, JsonValue>`, which already accepts arbitrary keys
/// (Rust's serde is permissive by default). This function exists to keep
/// the validation surface small — `additionalProperties: false` on
/// `params` is intentionally NOT enforced because individual steps
/// interpret `params` freely.
fn field_params(_field_name: &str, _purpose: &str) -> BTreeMap<String, JsonValue> {
    BTreeMap::new()
}

fn validate_object_keys(
    _value: &BTreeMap<String, JsonValue>,
    _allowed: &[&str],
    _errors: &mut Vec<String>,
    _path: &str,
) {
    // No-op — see `field_params`.
}

fn summarize_spec(spec: &CapabilitySpec) -> AuthorSpecSummary {
    AuthorSpecSummary {
        id: spec.id.clone(),
        display_name: spec.display_name.clone(),
        category: spec.category.clone(),
        status: spec.status.clone(),
        supported_engines: spec.supported_engines.clone(),
        config_fields: spec.config_schema.iter().map(|f| f.name.clone()).collect(),
        install_steps: spec.install.len(),
        uninstall_steps: spec.uninstall.len(),
        verify_checks: spec.verify.len(),
        safety_notes: spec.safety_notes.clone(),
    }
}

fn render_preview(summary: &AuthorSpecSummary, _yaml: &str) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "Capability: {} ({})",
        summary.id, summary.display_name
    ));
    lines.push(format!(
        "Category: {} | Status: {}",
        summary.category, summary.status
    ));
    if !summary.supported_engines.is_empty() {
        lines.push(format!(
            "Supported engines: {}",
            summary.supported_engines.join(", ")
        ));
    }
    if !summary.config_fields.is_empty() {
        lines.push(String::new());
        lines.push("Config fields the user will be prompted for:".to_owned());
        for field in &summary.config_fields {
            lines.push(format!("  - {field}"));
        }
    }
    if summary.install_steps > 0 {
        lines.push(String::new());
        lines.push(format!(
            "Install plan: {} step{}",
            summary.install_steps,
            if summary.install_steps == 1 { "" } else { "s" }
        ));
    }
    if summary.uninstall_steps > 0 {
        lines.push(format!(
            "Uninstall plan: {} step{}",
            summary.uninstall_steps,
            if summary.uninstall_steps == 1 { "" } else { "s" }
        ));
    }
    if summary.verify_checks > 0 {
        lines.push(format!(
            "Verify checks: {}",
            summary.verify_checks
        ));
    }
    if !summary.safety_notes.is_empty() {
        lines.push(String::new());
        lines.push("Safety notes:".to_owned());
        for note in &summary.safety_notes {
            lines.push(format!("  - {note}"));
        }
    }
    lines.push(String::new());
    lines.push(
        "Static preview only. The actual install runs through Moddin's transaction store with full rollback. The user must review and confirm before any file is written."
            .to_owned(),
    );
    lines.join("\n")
}

fn build_prompt_text(context: &AuthorPromptContext) -> String {
    // The `recommend` mode produces a fundamentally different prompt
    // (it's asking the AI to pick from a catalog, not to author one),
    // so we route it before the verbosity split.
    if matches!(context.mode, AuthorPromptMode::Recommend) {
        return match context.verbosity {
            AuthorPromptVerbosity::Basic => build_prompt_text_recommend_basic(context),
            AuthorPromptVerbosity::Advanced => build_prompt_text_recommend_advanced(context),
        };
    }
    match context.verbosity {
        AuthorPromptVerbosity::Basic => build_prompt_text_basic(context),
        AuthorPromptVerbosity::Advanced => build_prompt_text_advanced(context),
    }
}

/// Plain-Portuguese prompt aimed at a non-developer who already has
/// ChatGPT / Claude / Gemini open in another tab. Technical schema
/// terms are kept in parentheses so the AI still understands them.
fn build_prompt_text_basic(context: &AuthorPromptContext) -> String {
    let mut out = String::new();
    out.push_str("# Pedido para o Moddin Desktop\n\n");
    match context.mode {
        AuthorPromptMode::Author => {
            out.push_str(
                "Você está ajudando um usuário do Moddin Desktop a **criar um mod novo** para um jogo que ele já tem instalado.\n\n",
            );
            if let Some(game) = context.game_name.as_deref().or(context.game_id.as_deref()) {
                out.push_str(&format!("**Jogo alvo:** {game}\n\n"));
            }
            out.push_str("## O que o usuário quer (em linguagem simples)\n\n");
            out.push_str("> ");
            out.push_str(context.intent.as_deref().unwrap_or("(descreva aqui o que o mod deve fazer)"));
            out.push_str("\n\n");
        }
        AuthorPromptMode::Improve => {
            out.push_str(
                "Você está ajudando um usuário do Moddin Desktop a **mudar um mod que ele já tem instalado**.\n\n",
            );
            if let Some(id) = context.capability_id.as_deref() {
                out.push_str(&format!("**Mod atual:** `{id}`\n\n"));
            }
            out.push_str("## O que ele quer mudar\n\n");
            out.push_str("> ");
            out.push_str(context.intent.as_deref().unwrap_or("(descreva a mudança desejada)"));
            out.push_str("\n\n");
        }
        AuthorPromptMode::Improve => {
            out.push_str(
                "Você está ajudando um usuário do Moddin Desktop a **mudar um mod que ele já tem instalado**.\n\n",
            );
            if let Some(id) = context.capability_id.as_deref() {
                out.push_str(&format!("**Mod atual:** `{id}`\n\n"));
            }
            out.push_str("## O que ele quer mudar\n\n");
            out.push_str("> ");
            out.push_str(context.intent.as_deref().unwrap_or("(descreva a mudança desejada)"));
            out.push_str("\n\n");
        }
        AuthorPromptMode::Diagnose => {
            out.push_str(
                "Você está ajudando um usuário do Moddin Desktop a **entender por que algo deu errado**.\n\n",
            );
            if let Some(source) = context.error_source.as_deref() {
                out.push_str(&format!("**Onde apareceu o erro:** {source}\n\n"));
            }
            if let Some(game) = context.game_name.as_deref().or(context.game_id.as_deref()) {
                out.push_str(&format!("**Jogo:** {game}\n\n"));
            }
            if let Some(id) = context.capability_id.as_deref() {
                out.push_str(&format!("**Mod envolvido:** `{id}`\n\n"));
            }
            out.push_str("## Mensagem de erro\n\n```\n");
            out.push_str(context.error_message.as_deref().unwrap_or("(cole aqui a mensagem de erro)"));
            out.push_str("\n```\n\n");
            out.push_str("## O que o usuário já tentou\n\n");
            out.push_str("> ");
            out.push_str(context.intent.as_deref().unwrap_or("(descreva o que já foi tentado)"));
            out.push_str("\n\n");
        }
        AuthorPromptMode::Recommend => {
            // Routed in `build_prompt_text` before the verbosity split.
        }
    }

    out.push_str("## Como um mod do Moddin é descrito por dentro\n\n");
    out.push_str(
        "Pense em um mod (que o Moddin chama de *capability*) como uma **receita de bolo com etapas**:\n\n",
    );
    out.push_str(
        "- Tem um **nome** e um **apelido** (displayName, é o que aparece na interface).\n",
    );
    out.push_str("- Tem uma **categoria** (vr / graphics / qol / system) e um **status** (available / planned).\n");
    out.push_str("- Pode pedir algumas **informações para o usuário** antes de instalar (configSchema — campos que o Moddin pergunta na hora, como a URL de download ou o SHA-256 do arquivo).\n");
    out.push_str("- Tem **etapas de instalação** (install — passos que o Moddin executa, na ordem) e **etapas de remoção** (uninstall — para desfazer).\n");
    out.push_str("- Pode ter **verificações** (checks / verify — testes que o Moddin roda para saber se o mod está funcionando).\n\n");

    out.push_str("## Tipos de ação (step kinds) que o Moddin aceita\n\n");
    out.push_str("Cada etapa de install / uninstall precisa ter um `kind` (tipo de ação). Estes são todos os tipos válidos:\n\n");
    for kind in VALID_STEP_KINDS {
        out.push_str(&format!("- `{kind}`\n"));
    }
    out.push_str("\n## Tipos de verificação (check kinds)\n\n");
    out.push_str("As verificações (checks) também têm um `kind`. Estes são todos válidos:\n\n");
    for kind in VALID_CHECK_KINDS {
        out.push_str(&format!("- `{kind}`\n"));
    }
    out.push_str("\n## Regras de formato que o Moddin exige\n\n");
    out.push_str("- O campo `id` (apelido interno) tem que ser minúsculo, sem espaço e sem acento (ex.: `fps-unlocker`, `meu-mod-2`). Entre 2 e 64 caracteres.\n");
    out.push_str("- O `displayName` (nome visível) é o que o usuário vê — pode ter acento, maiúscula e espaço.\n");
    out.push_str("- `category` tem que ser exatamente um destes: `vr`, `graphics`, `qol`, `system`.\n");
    out.push_str("- `status` tem que ser `available` ou `planned`.\n");
    out.push_str("- Os campos que você referenciar em install/uninstall/checks **têm** que existir em `configSchema`.\n");
    out.push_str("- Para mexer no **Registro do Windows** (registry), só pode ser no HKCU (chaves do usuário atual). HKLM é bloqueado pelo Moddin.\n");
    out.push_str("- Se o mod baixa um arquivo, é obrigatório ter `downloadUrl` (URL HTTPS) e `sha256` (código de 64 caracteres que garante que o arquivo é o original). Não chute esses valores — deixe vazio se o usuário não passou.\n");

    out.push_str("\n## Exemplo completo (download de um zip)\n\n");
    out.push_str("Use este exemplo como ponto de partida. Ele baixa um arquivo .zip, verifica o SHA-256, e extrai na pasta do jogo:\n\n");
    out.push_str(EXAMPLE_YAML);

    out.push_str("\n## Formato da sua resposta\n\n");
    out.push_str(
        "Devolva **apenas** o YAML dentro de um bloco ```yaml (cercado por três crases). Nada antes, nada depois — sem explicações, sem comentários, sem markdown adicional. O Moddin vai validar esse YAML automaticamente.\n\n",
    );
    out.push_str(
        "Se a ideia do usuário não der pra fazer com os tipos de ação acima, explique em uma frase curta logo ANTES do bloco YAML por que não dá (em vez de inventar um tipo novo).",
    );

    out
}

/// Terse English prompt for developers / AI agents that already know
/// what a YAML capability schema is.
fn build_prompt_text_advanced(context: &AuthorPromptContext) -> String {
    let mode_label = match context.mode {
        AuthorPromptMode::Author => "Author a new Moddin capability",
        AuthorPromptMode::Improve => "Improve an existing Moddin capability",
        AuthorPromptMode::Diagnose => "Diagnose a Moddin error",
        AuthorPromptMode::Recommend => "Recommend Moddin capabilities",
    };
    let mut out = String::new();
    out.push_str("# Moddin capability ");
    out.push_str(match context.mode {
        AuthorPromptMode::Author => "author",
        AuthorPromptMode::Improve => "improve",
        AuthorPromptMode::Diagnose => "diagnose",
        AuthorPromptMode::Recommend => "recommend",
    });
    out.push_str(" request\n\n");
    out.push_str(&format!("You are helping a Moddin Desktop user **{mode_label}**.\n\n"));

    match context.mode {
        AuthorPromptMode::Author => {
            out.push_str("## Target\n\n");
            if let Some(game) = context.game_name.as_deref().or(context.game_id.as_deref()) {
                out.push_str(&format!("- Game: `{game}`\n"));
            }
            if let Some(id) = context.game_id.as_deref() {
                out.push_str(&format!("- Moddin gameId: `{id}`\n"));
            }
            out.push_str("\n## What the user wants\n\n");
            out.push_str(context.intent.as_deref().unwrap_or("(describe the mod here)"));
            out.push_str("\n");
        }
        AuthorPromptMode::Improve => {
            out.push_str("## Capability being improved\n\n");
            if let Some(id) = context.capability_id.as_deref() {
                out.push_str(&format!("- id: `{id}`\n"));
            }
            out.push_str("\n## Desired change\n\n");
            out.push_str(context.intent.as_deref().unwrap_or("(describe the change)"));
            out.push_str("\n");
        }
        AuthorPromptMode::Diagnose => {
            out.push_str("## Failure context\n\n");
            if let Some(source) = context.error_source.as_deref() {
                out.push_str(&format!("- Source: `{source}`\n"));
            }
            if let Some(game) = context.game_name.as_deref().or(context.game_id.as_deref()) {
                out.push_str(&format!("- Game: `{game}`\n"));
            }
            if let Some(id) = context.capability_id.as_deref() {
                out.push_str(&format!("- Capability: `{id}`\n"));
            }
            out.push_str("\n## Error message\n\n```\n");
            out.push_str(context.error_message.as_deref().unwrap_or("(paste the error)"));
            out.push_str("\n```\n");
            out.push_str("\n## What the user already knows / tried\n\n");
            out.push_str(context.intent.as_deref().unwrap_or("(describe what was tried)"));
            out.push_str("\n");
        }
        AuthorPromptMode::Recommend => {
            // Routed in `build_prompt_text` before the verbosity split.
        }
    }

    out.push_str("\n## Moddin capability schema (authoritative)\n\n");
    out.push_str(&schema_brief(context.mode));
    out.push_str("\n## Example (extract-zip is the most common pattern)\n\n");
    out.push_str(EXAMPLE_YAML);
    out.push_str("\n## Rules\n\n");
    out.push_str("- Return **only** the YAML inside a single ```yaml fenced block. No prose before or after the block.\n");
    out.push_str("- Every config field you reference from `install` / `uninstall` / `checks` must exist in `configSchema`.\n");
    out.push_str("- Never invent step or check kinds — only the enums above are accepted.\n");
    out.push_str("- For HKCU registry keys only. HKLM is rejected by Moddin's path guard.\n");
    out.push_str("- SHA-256 and downloadUrl are required for archive-based mods. Leave them empty only if the user explicitly opts out.\n");

    out
}

fn schema_brief(mode: AuthorPromptMode) -> String {
    let mut out = String::new();
    out.push_str("Required top-level fields: `id`, `displayName`, `category`, `status`.\n\n");
    out.push_str(&format!(
        "- `category` ∈ {VALID_CATEGORIES:?}\n- `status` ∈ {VALID_STATUSES:?}\n- `id` matches `^[a-z][a-z0-9-]{{1,63}}$`\n\n"
    ));
    out.push_str("Step kinds (the runner will reject any other value):\n");
    for kind in VALID_STEP_KINDS {
        out.push_str(&format!("- `{kind}`\n"));
    }
    out.push_str("\nCheck kinds:\n");
    for kind in VALID_CHECK_KINDS {
        out.push_str(&format!("- `{kind}`\n"));
    }
    if matches!(mode, AuthorPromptMode::Improve) {
        out.push_str(
            "\nWhen improving an existing capability, return the **complete** updated YAML — not a diff. Moddin replaces the file wholesale.\n",
        );
    }
    out
}

/// Plain Portuguese prompt for the `recommend` mode. The AI is given
/// the live catalog and asked to pick concrete items with confidence
/// scores.
fn build_prompt_text_recommend_basic(context: &AuthorPromptContext) -> String {
    let mut out = String::new();
    out.push_str("# Pedido de recomendação pro Moddin Desktop\n\n");
    out.push_str(
        "Você está ajudando um usuário do Moddin Desktop a **escolher quais mods instalar** pra atingir um objetivo.\n\n",
    );
    out.push_str("## O que o usuário quer fazer\n\n");
    if let Some(game) = context.game_name.as_deref().or(context.game_id.as_deref()) {
        out.push_str(&format!("**Jogo:** {game}\n\n"));
    }
    out.push_str("> ");
    out.push_str(context.intent.as_deref().unwrap_or("(descreva o objetivo)"));
    out.push_str("\n\n");

    out.push_str("## Catálogo disponível (escolha DELE, não invente)\n\n");
    if context.collection_catalog.is_empty() && context.capability_catalog.is_empty() {
        out.push_str("_Nenhum item no catálogo — diga ao usuário que não há nada pra recomendar._\n\n");
    } else {
        out.push_str("### Collections (pacotes prontos)\n");
        for entry in &context.collection_catalog {
            out.push_str(&format!(
                "- `{}` ({}): {}{}\n",
                entry.id,
                entry.display_name,
                entry.description,
                entry
                    .target_game
                    .as_deref()
                    .map(|g| format!(" — alvo: {g}"))
                    .unwrap_or_default()
            ));
        }
        out.push_str("\n### Capabilities individuais\n");
        for entry in &context.capability_catalog {
            out.push_str(&format!(
                "- `{}` ({} · {}): {}{}\n",
                entry.id,
                entry.display_name,
                entry.category,
                entry.description,
                entry
                    .target_game
                    .as_deref()
                    .map(|g| format!(" — alvo: {g}"))
                    .unwrap_or_default()
            ));
        }
        out.push_str("\n");
    }

    out.push_str("## O que você deve devolver\n\n");
    out.push_str(
        "Devolva **apenas** o YAML abaixo dentro de um bloco ```yaml (cercado por três crases). Nada antes, nada depois.\n\n",
    );
    out.push_str("O YAML deve ser uma lista ordenada de recomendações. Para cada item:\n\n");
    out.push_str("- `type`: `collection` (pacote pronto, recomendado quando bate com o pedido) ou `capability` (mod individual, pra completar ou ser mais cirúrgico).\n");
    out.push_str("- `id`: o identificador EXATO do catálogo acima. **Não invente ids.**\n");
    out.push_str("- `reason` (em português): uma frase curta explicando por que esse item é uma boa escolha pro pedido.\n");
    out.push_str("- `confidence`: um número de 0 a 1 indicando o quanto você recomenda. Acima de 0.7 = muito recomendado; 0.4-0.7 = opcional; abaixo de 0.4 = não inclua.\n\n");
    out.push_str("Formato:\n\n");
    out.push_str("```yaml\n");
    out.push_str("recommendations:\n");
    out.push_str("  - type: collection\n");
    out.push_str("    id: <id-do-catalogo>\n");
    out.push_str("    reason: <frase-curta>\n");
    out.push_str("    confidence: <0-1>\n");
    out.push_str("  - type: capability\n");
    out.push_str("    id: <id-do-catalogo>\n");
    out.push_str("    reason: <frase-curta>\n");
    out.push_str("    confidence: <0-1>\n");
    out.push_str("```\n\n");
    out.push_str(
        "Se o catálogo não tiver nada que combine, devolva uma lista vazia (`recommendations: []`) — não invente itens.",
    );
    out
}

/// Terse English prompt for the `recommend` mode. Same shape as the
/// basic version, aimed at AI agents / developers.
fn build_prompt_text_recommend_advanced(context: &AuthorPromptContext) -> String {
    let mut out = String::new();
    out.push_str("# Moddin capability recommendation request\n\n");
    out.push_str("Pick the best Moddin capabilities / collections for the user's goal.\n\n");

    out.push_str("## User goal\n\n");
    if let Some(game) = context.game_name.as_deref().or(context.game_id.as_deref()) {
        out.push_str(&format!("- Game: `{game}`\n"));
    }
    out.push_str("- Goal: ");
    out.push_str(context.intent.as_deref().unwrap_or("(describe the goal)"));
    out.push_str("\n\n");

    out.push_str("## Available catalog\n\n");
    if context.collection_catalog.is_empty() && context.capability_catalog.is_empty() {
        out.push_str("_Catalog is empty — return `recommendations: []`._\n\n");
    } else {
        out.push_str("### Collections\n");
        for entry in &context.collection_catalog {
            out.push_str(&format!(
                "- `{}` ({}): {}{}\n",
                entry.id,
                entry.display_name,
                entry.description,
                entry
                    .target_game
                    .as_deref()
                    .map(|g| format!(" — target: {g}"))
                    .unwrap_or_default()
            ));
        }
        out.push_str("\n### Capabilities\n");
        for entry in &context.capability_catalog {
            out.push_str(&format!(
                "- `{}` ({} · {}): {}{}\n",
                entry.id,
                entry.display_name,
                entry.category,
                entry.description,
                entry
                    .target_game
                    .as_deref()
                    .map(|g| format!(" — target: {g}"))
                    .unwrap_or_default()
            ));
        }
        out.push_str("\n");
    }

    out.push_str("## Output format\n\n");
    out.push_str(
        "Return **only** a YAML block (```yaml fences, nothing else) with shape:\n\n",
    );
    out.push_str("```yaml\nrecommendations:\n  - type: collection|capability\n    id: <catalog-id>\n    reason: <one-line>\n    confidence: <0-1>\n```\n\n");
    out.push_str("Rules:\n- Use only ids from the catalog.\n- confidence >= 0.4 to include; below that, omit.\n- Empty list (`recommendations: []`) is a valid answer.\n");

    out
}

const EXAMPLE_YAML: &str = r#"```yaml
id: my-mod-extract
displayName: My Mod
category: graphics
status: available
supportedEngines: []
configSchema:
  - name: downloadUrl
    type: url
    required: true
    description: HTTPS URL of the mod archive.
  - name: sha256
    type: sha256
    required: true
    description: Expected SHA-256 of the archive.
  - name: version
    type: string
    required: true
    description: Upstream release tag.
  - name: expectedFile
    type: path
    required: true
    description: Relative path (from executableDir) of one critical file the archive must contain.
checks:
  - id: archive-reachable
    label: Archive reachable
    kind: archive-reachable
    severity: warning
    category: modulespecific
    params:
      urlField: downloadUrl
  - id: archive-sha256
    label: Archive SHA-256 matches
    kind: archive-sha256
    severity: blocker
    category: modulespecific
    params:
      urlField: downloadUrl
      expectedField: sha256
install:
  - kind: extract-zip
    description: Extract the archive into the game's executable directory.
    params:
      archivePathField: downloadUrl
uninstall:
  - kind: file-delete
    description: Remove the critical file to roll back the install.
    params:
      pathField: expectedFile
safetyNotes:
  - Close the game (and any overlays) before applying or removing.
```"#;

// ---------------------------------------------------------------------------
// Registry hot-reload
// ---------------------------------------------------------------------------

fn reload_registry_from(dir: &PathBuf) {
    static STATE: OnceLock<Mutex<CapabilityRegistry>> = OnceLock::new();
    let mutex = STATE.get_or_init(|| Mutex::new(CapabilityRegistry::load()));
    if let Ok(mut registry) = mutex.lock() {
        registry.reload_local(dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_id_pattern() {
        // Mirrors the JSON schema `^[a-z][a-z0-9-]{1,63}$`. The
        // schema (and the JS agent that validates against it) accepts
        // trailing dashes — we must match that behavior so a YAML the
        // external MCP server already validated does not get rejected
        // by the in-app validator.
        assert!(is_valid_capability_id("optiscaler"));
        assert!(is_valid_capability_id("a-b-c-1"));
        assert!(is_valid_capability_id("trailing-dash-"));
        assert!(!is_valid_capability_id("A"));
        assert!(!is_valid_capability_id("-leading-dash"));
        assert!(!is_valid_capability_id(""));
        assert!(!is_valid_capability_id("a"));
        assert!(!is_valid_capability_id(&"a".repeat(65)));
    }

    #[test]
    fn check_id_pattern() {
        assert!(is_valid_check_id("archive-sha256"));
        assert!(!is_valid_check_id("ArchiveSha"));
        assert!(!is_valid_check_id("a"));
    }

    #[test]
    fn config_field_name_pattern() {
        assert!(is_valid_config_field_name("downloadUrl"));
        assert!(is_valid_config_field_name("Sha_256"));
        assert!(!is_valid_config_field_name("1leading"));
        assert!(!is_valid_config_field_name("kebab-case"));
    }

    #[test]
    fn rejects_unknown_root_keys() {
        let raw = serde_json::json!({
            "id": "demo",
            "displayName": "Demo",
            "category": "qol",
            "status": "available",
            "bogus": true,
        });
        let mut errors = Vec::new();
        validate_root(&raw, &mut errors);
        assert!(errors.iter().any(|e| e.contains("bogus")));
    }

    #[test]
    fn rejects_missing_required() {
        let raw = serde_json::json!({
            "id": "demo",
            "displayName": "Demo",
        });
        let mut errors = Vec::new();
        validate_root(&raw, &mut errors);
        assert!(errors.iter().any(|e| e.contains("category")));
        assert!(errors.iter().any(|e| e.contains("status")));
    }

    #[test]
    fn rejects_bad_category() {
        let yaml = r#"
id: demo
displayName: Demo
category: shooter
status: available
"#;
        let result = validate_yaml_string(yaml);
        assert!(!result.ok);
        assert!(result.errors.iter().any(|e| e.contains("category")));
    }

    #[test]
    fn accepts_minimal_valid_spec() {
        let yaml = r#"
id: demo
displayName: Demo
category: qol
status: available
"#;
        let result = validate_yaml_string(yaml);
        assert!(result.ok, "errors: {:?}", result.errors);
        let summary = result.spec.expect("summary present");
        assert_eq!(summary.id, "demo");
        assert_eq!(summary.config_fields.len(), 0);
        assert_eq!(summary.install_steps, 0);
    }

    #[test]
    fn rejects_unknown_step_kind() {
        let yaml = r#"
id: demo
displayName: Demo
category: qol
status: available
install:
  - kind: rm-rf
"#;
        let result = validate_yaml_string(yaml);
        assert!(!result.ok);
        assert!(result.errors.iter().any(|e| e.contains("kind")));
    }

    #[test]
    fn rejects_duplicate_config_field() {
        let yaml = r#"
id: demo
displayName: Demo
category: qol
status: available
configSchema:
  - name: foo
    type: string
  - name: foo
    type: string
"#;
        let result = validate_yaml_string(yaml);
        assert!(!result.ok);
        assert!(result.errors.iter().any(|e| e.contains("duplicate")));
    }

    #[test]
    fn prompt_includes_intent_and_example() {
        let ctx = AuthorPromptContext {
            mode: AuthorPromptMode::Author,
            verbosity: AuthorPromptVerbosity::Advanced,
            game_id: Some("elden-ring".to_owned()),
            game_name: Some("Elden Ring".to_owned()),
            capability_id: None,
            intent: Some("an FPS unlocker".to_owned()),
            error_message: None,
            error_source: None,
            ..AuthorPromptContext::default()
        };
        let prompt = build_author_prompt(ctx);
        assert!(prompt.contains("elden-ring"));
        assert!(prompt.contains("FPS unlocker"));
        assert!(prompt.contains("extract-zip"));
    }

    #[test]
    fn prompt_diagnose_includes_error() {
        let ctx = AuthorPromptContext {
            mode: AuthorPromptMode::Diagnose,
            verbosity: AuthorPromptVerbosity::Advanced,
            game_id: Some("elden-ring".to_owned()),
            game_name: None,
            capability_id: Some("ofxr-bridge".to_owned()),
            intent: Some("Tried twice, second time got the same error".to_owned()),
            error_message: Some("archive-sha256 mismatch".to_owned()),
            error_source: Some("module install".to_owned()),
            ..AuthorPromptContext::default()
        };
        let prompt = build_author_prompt(ctx);
        assert!(prompt.contains("archive-sha256 mismatch"));
        assert!(prompt.contains("ofxr-bridge"));
    }

    #[test]
    fn default_verbosity_is_basic() {
        assert_eq!(AuthorPromptVerbosity::default(), AuthorPromptVerbosity::Basic);
    }

    #[test]
    fn basic_prompt_is_in_portuguese_with_terms() {
        let ctx = AuthorPromptContext {
            mode: AuthorPromptMode::Author,
            verbosity: AuthorPromptVerbosity::Basic,
            game_id: Some("elden-ring".to_owned()),
            game_name: Some("Elden Ring".to_owned()),
            capability_id: None,
            intent: Some("um destravador de FPS".to_owned()),
            error_message: None,
            error_source: None,
            ..AuthorPromptContext::default()
        };
        let prompt = build_author_prompt(ctx);
        // Friendly language
        assert!(prompt.contains("Pedido para o Moddin Desktop"));
        assert!(prompt.contains("linguagem simples"));
        assert!(prompt.contains("destravador de FPS"));
        // Technical terms kept in parentheses for the AI
        assert!(prompt.contains("displayName"));
        assert!(prompt.contains("configSchema"));
        assert!(prompt.contains("registry"));
        assert!(prompt.contains("HKCU"));
        // Schema enums still present so the AI produces valid YAML
        assert!(prompt.contains("extract-zip"));
        assert!(prompt.contains("archive-sha256"));
    }

    #[test]
    fn advanced_prompt_is_english_and_terse() {
        let ctx = AuthorPromptContext {
            mode: AuthorPromptMode::Author,
            verbosity: AuthorPromptVerbosity::Advanced,
            game_id: Some("elden-ring".to_owned()),
            game_name: Some("Elden Ring".to_owned()),
            capability_id: None,
            intent: Some("an FPS unlocker".to_owned()),
            error_message: None,
            error_source: None,
            ..AuthorPromptContext::default()
        };
        let prompt = build_author_prompt(ctx);
        assert!(prompt.contains("# Moddin capability author request"));
        assert!(prompt.contains("Moddin capability schema (authoritative)"));
        assert!(prompt.contains("extract-zip"));
        // Portuguese-friendly terms should NOT appear in advanced mode
        assert!(!prompt.contains("Pedido para o Moddin Desktop"));
        assert!(!prompt.contains("linguagem simples"));
    }

    #[test]
    fn basic_diagnose_prompt_uses_pt_br() {
        let ctx = AuthorPromptContext {
            mode: AuthorPromptMode::Diagnose,
            verbosity: AuthorPromptVerbosity::Basic,
            game_id: Some("elden-ring".to_owned()),
            game_name: None,
            capability_id: Some("ofxr-bridge".to_owned()),
            intent: Some("tentei duas vezes".to_owned()),
            error_message: Some("arquivo corrompido".to_owned()),
            error_source: Some("install".to_owned()),
            ..AuthorPromptContext::default()
        };
        let prompt = build_author_prompt(ctx);
        assert!(prompt.contains("entender por que algo deu errado"));
        assert!(prompt.contains("arquivo corrompido"));
        assert!(prompt.contains("ofxr-bridge"));
    }

    #[test]
    fn basic_prompt_default_when_not_specified() {
        let ctx = AuthorPromptContext {
            mode: AuthorPromptMode::Author,
            // verbosity omitted on purpose — serde default should kick in
            ..AuthorPromptContext::default()
        };
        let prompt = build_author_prompt(ctx);
        // Basic is the default → should look like the friendly PT-BR prompt
        assert!(prompt.contains("Pedido para o Moddin Desktop"));
    }

    #[test]
    fn recommend_basic_prompt_includes_catalog() {
        let ctx = AuthorPromptContext {
            mode: AuthorPromptMode::Recommend,
            verbosity: AuthorPromptVerbosity::Basic,
            intent: Some("Cyberpunk em VR".to_owned()),
            capability_catalog: vec![CatalogCapability {
                id: "optiscaler".to_owned(),
                display_name: "OptiScaler".to_owned(),
                category: "graphics".to_owned(),
                description: "Upscaling para qualquer jogo".to_owned(),
                target_game: None,
            }],
            collection_catalog: vec![CatalogCollection {
                id: "vr-cyberpunk-essential".to_owned(),
                display_name: "VR Cyberpunk — Essencial".to_owned(),
                category: "vr".to_owned(),
                description: "Pacote VR pronto".to_owned(),
                target_game: Some("cyberpunk-2077".to_owned()),
                capability_count: 4,
            }],
            ..AuthorPromptContext::default()
        };
        let prompt = build_author_prompt(ctx);
        assert!(prompt.contains("Pedido de recomendação"));
        assert!(prompt.contains("Cyberpunk em VR"));
        assert!(prompt.contains("vr-cyberpunk-essential"));
        assert!(prompt.contains("OptiScaler"));
        assert!(prompt.contains("confidence"));
    }

    #[test]
    fn recommend_advanced_prompt_includes_catalog() {
        let ctx = AuthorPromptContext {
            mode: AuthorPromptMode::Recommend,
            verbosity: AuthorPromptVerbosity::Advanced,
            intent: Some("Elden Ring VR".to_owned()),
            capability_catalog: vec![],
            collection_catalog: vec![CatalogCollection {
                id: "vr-elden-ring-starter".to_owned(),
                display_name: "VR Elden Ring — Starter".to_owned(),
                category: "vr".to_owned(),
                description: "Pacote VR básico".to_owned(),
                target_game: Some("elden-ring".to_owned()),
                capability_count: 3,
            }],
            ..AuthorPromptContext::default()
        };
        let prompt = build_author_prompt(ctx);
        assert!(prompt.contains("# Moddin capability recommendation request"));
        assert!(prompt.contains("vr-elden-ring-starter"));
    }

    #[test]
    fn recommend_prompt_with_empty_catalog_handles_gracefully() {
        let ctx = AuthorPromptContext {
            mode: AuthorPromptMode::Recommend,
            intent: Some("qualquer coisa".to_owned()),
            ..AuthorPromptContext::default()
        };
        let prompt = build_author_prompt(ctx);
        // Should still produce a valid prompt, not panic
        assert!(prompt.contains("Catálogo disponível"));
    }

    #[test]
    fn validate_recommendations_accepts_valid_list() {
        let yaml = r#"
- type: collection
  id: vr-cyberpunk-essential
  reason: Pacote pronto.
  confidence: 0.9
- type: capability
  id: optiscaler
  reason: Upscaling.
  confidence: 0.7
"#;
        let result = validate_recommendations_string(yaml);
        assert!(result.ok, "errors: {:?}", result.errors);
        assert_eq!(result.recommendations.len(), 2);
        assert_eq!(result.recommendations[0].id, "vr-cyberpunk-essential");
        assert!((result.recommendations[0].confidence - 0.9).abs() < f32::EPSILON);
    }

    #[test]
    fn validate_recommendations_drops_low_confidence() {
        let yaml = r#"
- type: capability
  id: maybe-this
  reason: Incerto.
  confidence: 0.3
"#;
        let result = validate_recommendations_string(yaml);
        assert!(result.ok);
        assert!(result.recommendations.is_empty());
    }

    #[test]
    fn validate_recommendations_rejects_duplicate() {
        let yaml = r#"
- type: capability
  id: optiscaler
  reason: First.
  confidence: 0.8
- type: capability
  id: optiscaler
  reason: Second.
  confidence: 0.7
"#;
        let result = validate_recommendations_string(yaml);
        assert!(!result.ok);
        assert!(result.errors.iter().any(|e| e.contains("duplicates")));
    }

    #[test]
    fn validate_recommendations_rejects_out_of_range_confidence() {
        let yaml = r#"
- type: capability
  id: optiscaler
  reason: Test.
  confidence: 1.5
"#;
        let result = validate_recommendations_string(yaml);
        assert!(!result.ok);
        assert!(result.errors.iter().any(|e| e.contains("between 0 and 1")));
    }

    #[test]
    fn validate_recommendations_rejects_empty_list_silently() {
        let yaml = "[]";
        let result = validate_recommendations_string(yaml);
        assert!(result.ok);
        assert!(result.recommendations.is_empty());
    }

    #[test]
    fn validate_recommendations_rejects_garbage() {
        let yaml = "not a list at all";
        let result = validate_recommendations_string(yaml);
        assert!(!result.ok);
        assert!(!result.errors.is_empty());
    }
}
