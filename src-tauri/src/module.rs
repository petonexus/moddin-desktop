//! Shared contract for reusable Moddin modules.
//!
//! Every native capability that can be applied to a catalogued game should
//! eventually implement this trait. The trait is intentionally narrow: it
//! captures the lifecycle every Moddin module already exposes through
//! `preview_*`, `install_*`, `configure_*`, `uninstall_*` and
//! `check_module_update` Tauri commands, without dictating how the work is
//! done.
//!
//! ## Why a trait now
//!
//! Each existing module (`obs`, `optiscaler`, `openxr`, `ofxr`, `uevr`,
//! `cheeky`, `vr_launch`, `desktop_shortcut`) reimplements the same shape:
//!
//! - a request struct with the catalog-declared config;
//! - a preview that inspects the game directory and reports preconditions;
//! - an apply/install that performs a real mutation and records a
//!   `TransactionRecord` for rollback;
//! - a read-only verification pass;
//! - a removal / uninstall path that records another transaction;
//! - optionally, an update check.
//!
//! The trait below captures that shape so future modules can reuse the
//! transaction store, the preview/verify protocol, and the update signal
//! without re-inventing them.
//!
//! ## Adoption
//!
//! The current modules do **not** implement this trait yet — adopting it
//! is an incremental, per-module refactor (see `ROADMAP.md`). The trait
//! itself is the source of truth for what those refactors need to look
//! like.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::transaction::TransactionRecord;

/// Coarse classification of a module. Matches the `category` enum on the
/// TypeScript `ToolModuleDefinition` in `src/types/game.ts`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ModuleCategory {
    /// VR-specific capability (UEVR, OFXR, OpenXR helpers, VR launch, …).
    Vr,
    /// Graphics pipeline (OptiScaler, Cheeky, ReShade, …).
    Graphics,
    /// Quality-of-life (mod loaders, gameplay tweaks, …).
    Qol,
    /// System-level tooling (desktop shortcut, app-level helpers, …).
    System,
}

impl ModuleCategory {
    /// Stable identifier used in the YAML catalog.
    pub fn as_str(&self) -> &'static str {
        match self {
            ModuleCategory::Vr => "vr",
            ModuleCategory::Graphics => "graphics",
            ModuleCategory::Qol => "qol",
            ModuleCategory::System => "system",
        }
    }
}

/// Inputs every module receives from the desktop UI. Constructed by the
/// Tauri command layer from the catalog recipe + the selected installed
/// game. Path values must already be resolved and validated by the caller
/// (no module should re-validate the game root).
///
/// `config` carries the merged module-level recipe config (from the
/// per-game YAML entry and any engine preset fallback) as a JSON object.
/// Modules that need typed config fields (e.g. ReShade's `proxy`,
/// `archiveUrl`, `sha256`) translate it into their own request struct.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleContext {
    pub game_id: String,
    pub game_name: String,
    pub install_dir: PathBuf,
    pub executable: String,
    pub work_dir: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<serde_json::Value>,
}

/// Result of a dry-run `preview`. Reports the concrete preconditions the
/// UI surfaces as a checklist.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewReport {
    pub can_apply: bool,
    pub changes: Vec<String>,
    pub warnings: Vec<String>,
}

/// Where a check belongs in the UI's three-layer layout.
///
/// The desktop UI groups checks under three headings:
///
/// * `Global` — checks every Moddin module evaluates (game executable
///   present, game not running, executable directory writable).
///   Rendered once at the top of every module card.
/// * `Category` — checks that apply to every module in the same
///   category (VR runtime present for VR modules, GPU + driver for
///   Graphics modules). Rendered between the global heading and the
///   module-specific heading.
/// * `ModuleSpecific` — checks that only this module knows how to
///   evaluate (proxy DLL available, OFXR tray running, UEVR engine
///   version). Rendered last, and only on this module's card.
///
/// Modules must tag every `CheckOutcome` they push so the UI can
/// group them automatically.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum CheckCategory {
    #[default]
    ModuleSpecific,
    Category,
    Global,
}

/// How strongly a failed check should affect `can_apply` and the
/// rendered severity. The desktop UI maps this to colour and icon.
///
/// * `Info` — pass / fail is informational. Never blocks apply.
/// * `Warning` — failure surfaces a warning banner. Does not block apply
///   on its own (the module author decides whether to combine with
///   `Blocker` checks).
/// * `Blocker` — when failed, the module cannot be applied; the UI
///   disables the Apply button.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum CheckSeverity {
    #[default]
    Info,
    Warning,
    Blocker,
}

/// Declarative description of a check the module knows how to run. The
/// module returns one of these per logical check from
/// [`Module::verification_definitions`]; the runtime evaluates them
/// and reports each result as a [`CheckOutcome`] in
/// `VerificationReport.checks`.
///
/// `definitions` are the **schema**; `checks` are the **result**. The
/// UI uses `definitions` to know what to render even before any check
/// has been evaluated (e.g. right after selecting a game), and uses
/// `checks` to colour each row.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckDefinition {
    /// Stable id, e.g. `"game-not-running"`, `"proxy-available"`. The
    /// UI keys the rendered row on this so re-evaluation does not
    /// flicker the layout.
    pub id: &'static str,
    /// Human-readable label, e.g. `"Game not running"`.
    pub label: &'static str,
    /// Where the row belongs in the UI three-layer layout.
    #[serde(default)]
    pub category: CheckCategory,
    /// Severity when failed. Drives button enablement and banner colour.
    #[serde(default)]
    pub severity: CheckSeverity,
    /// Short helper text shown under the label when the check is in its
    /// default state. Use this to explain *what* the check means so the
    /// user is not surprised by failure detail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<&'static str>,
}

/// A single line item inside a `VerificationReport`. Mirrors the
/// TypeScript `ModuleVerificationCheck` shape used by the UI.
///
/// `id` lets the UI reconcile each result with the matching entry in
/// `VerificationReport.definitions`. Modules that don't carry an `id`
/// (older code) are tolerated by the UI — the UI falls back to a
/// positional match.
///
/// `Default` is derived so new fields can be added without breaking
/// call sites — older code uses `..Default::default()` and gets the
/// standard `ModuleSpecific` / `Info` defaults.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckOutcome {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<&'static str>,
    pub label: String,
    pub passed: bool,
    pub detail: Option<String>,
    #[serde(default)]
    pub category: CheckCategory,
    #[serde(default)]
    pub severity: CheckSeverity,
}

/// Read-only state snapshot. Drives the UI checklist and gates
/// mutating actions (a module that is not "ready" should not be applied).
///
/// `Default` is derived so callers can use `..Default::default()` and
/// pick up the standard empty `definitions` and `checks` vectors.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerificationReport {
    pub status: ModuleStatus,
    pub summary: String,
    pub checks: Vec<CheckOutcome>,
    pub game_running: bool,
    pub installed: bool,
    pub installed_version: Option<String>,
    /// Declarative schema of the checks the module can evaluate. The
    /// UI renders one row per definition even before any check has
    /// been run, then fills in the live `CheckOutcome` values as they
    /// arrive. Modules may omit this when no static metadata exists.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub definitions: Vec<CheckDefinition>,
}

/// Coarse state of a module for a specific game. Mirrors
/// `ModuleVerificationStatus` on the TypeScript side.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ModuleStatus {
    #[default]
    Unknown,
    Ready,
    Installed,
    Attention,
}

/// Result of `update_check`. Mirrors `ModuleUpdate` on the TypeScript side.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub status: UpdateStatus,
    pub current_version: Option<String>,
    pub latest_version: Option<String>,
    pub release_url: Option<String>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum UpdateStatus {
    Unknown,
    Current,
    Available,
    Unavailable,
    Error,
}

/// What `apply` and `remove` return to the UI. `transaction` is the
/// canonical undo handle; the rest are module-specific flags so the UI can
/// refresh without a second `verify` call.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyResult {
    pub transaction: TransactionRecord,
    pub installed: bool,
    pub started: bool,
    pub armed: bool,
    pub version: Option<String>,
    pub backend: Option<String>,
}

/// Shared lifecycle every Moddin module exposes.
///
/// All methods take a `ModuleContext` so they remain stateless. Modules
/// are free to hold their own state through the existing `*_marker.json`
/// files under `%LOCALAPPDATA%/Moddin/tools/<module>/`.
///
/// ## Async
///
/// `preview`, `apply`, `verify`, `remove`, and `update_check` are async
/// to match the existing `#[tauri::command]` shape — most modules need
/// to hit the network (download, update check) or the filesystem in a
/// non-blocking fashion. `rollback` stays sync because the transaction
/// store is fully synchronous.
///
/// ## Errors
///
/// Every method returns `Result<_, String>` because the existing Tauri
/// command boundary expects stringly-typed errors. The convention is:
///
/// * validation / safety problems → `Err(...)` so the UI surfaces them;
/// * recoverable "not ready yet" conditions → `Ok` with
///   `can_apply: false` or `status: Attention`.
pub trait Module: Send + Sync {
    /// Stable catalog id (e.g. `"obs-vr"`, `"ofxr-framegen"`, `"uevr"`).
    fn id(&self) -> &'static str;

    /// Display name shown in the UI.
    fn name(&self) -> &'static str;

    /// Coarse classification — drives the catalog category enum.
    fn category(&self) -> ModuleCategory;

    /// Dry-run: verify preconditions and list the planned changes.
    /// Must not mutate state.
    fn preview(
        &self,
        context: &ModuleContext,
    ) -> impl std::future::Future<Output = Result<PreviewReport, String>> + Send;

    /// Apply the module: install, configure, or launch as appropriate.
    /// Must record a `TransactionRecord` so the action is reversible.
    fn apply(
        &self,
        context: &ModuleContext,
    ) -> impl std::future::Future<Output = Result<ApplyResult, String>> + Send;

    /// Read-only state snapshot. Drives the verification checklist.
    /// Must not mutate state.
    fn verify(
        &self,
        context: &ModuleContext,
    ) -> impl std::future::Future<Output = Result<VerificationReport, String>> + Send;

    /// Declarative metadata of every check this module knows how to
    /// run. The UI uses this to render the verification list before
    /// any evaluation has happened (e.g. right after a game is
    /// selected) and as the schema for the live `CheckOutcome`s in
    /// `VerificationReport.checks`.
    ///
    /// Modules should return **all** checks they will ever evaluate,
    /// even ones that were skipped this round, so the UI layout is
    /// stable. Returning an empty slice is fine for modules that have
    /// not yet adopted this contract — the UI will fall back to a
    /// purely result-driven render.
    fn verification_definitions(&self) -> &'static [CheckDefinition] {
        &[]
    }

    /// Undo a previously-recorded transaction. The default implementation
    /// delegates to the transaction store, which already knows how to
    /// restore files for `apply`-style transactions. Returns the
    /// post-rollback `TransactionRecord` so the UI can refresh its view.
    fn rollback(&self, transaction_id: &str) -> Result<TransactionRecord, String> {
        crate::transaction::rollback_transaction(transaction_id.to_owned())
    }

    /// Reversibly tear the module down. Distinct from `rollback`: it
    /// removes everything the module installed, not just the latest
    /// transaction. Must also record a `TransactionRecord`.
    fn remove(
        &self,
        context: &ModuleContext,
    ) -> impl std::future::Future<Output = Result<ApplyResult, String>> + Send;

    /// Optional upstream-version check. The default returns
    /// `UpdateStatus::Unknown` so modules without a public release feed
    /// don't have to implement anything.
    fn update_check(
        &self,
        context: &ModuleContext,
    ) -> impl std::future::Future<Output = Result<UpdateInfo, String>> + Send {
        let _ = context;
        async move {
            Ok(UpdateInfo {
                status: UpdateStatus::Unknown,
                current_version: None,
                latest_version: None,
                release_url: None,
                detail: None,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DummyModule;

    impl Module for DummyModule {
        fn id(&self) -> &'static str {
            "dummy"
        }
        fn name(&self) -> &'static str {
            "Dummy"
        }
        fn category(&self) -> ModuleCategory {
            ModuleCategory::System
        }
        async fn preview(&self, _context: &ModuleContext) -> Result<PreviewReport, String> {
            Ok(PreviewReport {
                can_apply: true,
                changes: Vec::new(),
                warnings: Vec::new(),
            })
        }
        async fn apply(&self, _context: &ModuleContext) -> Result<ApplyResult, String> {
            Err("apply not implemented in test stub".to_owned())
        }
        async fn verify(&self, _context: &ModuleContext) -> Result<VerificationReport, String> {
            Ok(VerificationReport {
                status: ModuleStatus::Unknown,
                summary: "test stub".to_owned(),
                checks: Vec::new(),
                game_running: false,
                installed: false,
                installed_version: None,
                definitions: Vec::new(),
            })
        }
        async fn remove(&self, _context: &ModuleContext) -> Result<ApplyResult, String> {
            Err("remove not implemented in test stub".to_owned())
        }
    }

    #[test]
    fn dummy_module_reports_its_identity() {
        let module = DummyModule;
        assert_eq!(module.id(), "dummy");
        assert_eq!(module.name(), "Dummy");
        assert_eq!(module.category(), ModuleCategory::System);
        assert_eq!(module.category().as_str(), "system");
    }

    #[tokio::test]
    async fn dummy_module_defaults_update_check_to_unknown() {
        let module = DummyModule;
        let context = ModuleContext {
            game_id: "test".to_owned(),
            game_name: "Test".to_owned(),
            install_dir: PathBuf::from("C:/Games/Test"),
            executable: "test.exe".to_owned(),
            work_dir: PathBuf::from("C:/Users/me/AppData/Local/Moddin/tools/dummy"),
            config: None,
        };
        let info = module.update_check(&context).await.expect("default update_check");
        assert_eq!(info.status, UpdateStatus::Unknown);
        assert!(info.latest_version.is_none());
    }

    #[test]
    fn check_category_and_severity_default_to_module_specific_info() {
        let outcome = CheckOutcome {
            id: None,
            label: "Test".to_owned(),
            passed: true,
            detail: None,
            category: CheckCategory::default(),
            severity: CheckSeverity::default(),
        };
        let serialized = serde_json::to_string(&outcome).expect("serialize");
        assert!(serialized.contains("\"category\":\"modulespecific\""));
        assert!(serialized.contains("\"severity\":\"info\""));
    }

    #[test]
    fn check_outcome_omits_id_when_none() {
        let outcome = CheckOutcome {
            id: None,
            label: "Test".to_owned(),
            passed: true,
            detail: None,
            category: CheckCategory::Global,
            severity: CheckSeverity::Blocker,
        };
        let serialized = serde_json::to_string(&outcome).expect("serialize");
        assert!(!serialized.contains("\"id\""));
        assert!(serialized.contains("\"category\":\"global\""));
        assert!(serialized.contains("\"severity\":\"blocker\""));
    }

    #[test]
    fn check_definition_carries_static_metadata() {
        let definition = CheckDefinition {
            id: "proxy-available",
            label: "Proxy DLL available",
            category: CheckCategory::ModuleSpecific,
            severity: CheckSeverity::Blocker,
            description: Some("The chosen proxy DLL must not already exist in the game directory."),
        };
        let serialized = serde_json::to_string(&definition).expect("serialize");
        assert!(serialized.contains("\"id\":\"proxy-available\""));
        assert!(serialized.contains("\"label\":\"Proxy DLL available\""));
        assert!(serialized.contains("\"category\":\"modulespecific\""));
        assert!(serialized.contains("\"severity\":\"blocker\""));
        assert!(serialized.contains("\"description\""));
    }

    #[test]
    fn verification_report_definitions_default_to_empty() {
        let report = VerificationReport {
            status: ModuleStatus::Ready,
            summary: "ok".to_owned(),
            checks: Vec::new(),
            game_running: false,
            installed: true,
            installed_version: None,
            definitions: Vec::new(),
        };
        let serialized = serde_json::to_string(&report).expect("serialize");
        assert!(!serialized.contains("definitions"));
    }
}