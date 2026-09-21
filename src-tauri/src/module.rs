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
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleContext {
    pub game_id: String,
    pub game_name: String,
    pub install_dir: PathBuf,
    pub executable: String,
    pub work_dir: PathBuf,
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

/// A single line item inside a `VerificationReport`. Mirrors the
/// TypeScript `ModuleVerificationCheck` shape used by the UI.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckOutcome {
    pub label: String,
    pub passed: bool,
    pub detail: Option<String>,
}

/// Read-only state snapshot. Drives the UI checklist and gates
/// mutating actions (a module that is not "ready" should not be applied).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerificationReport {
    pub status: ModuleStatus,
    pub summary: String,
    pub checks: Vec<CheckOutcome>,
    pub game_running: bool,
    pub installed: bool,
    pub installed_version: Option<String>,
}

/// Coarse state of a module for a specific game. Mirrors
/// `ModuleVerificationStatus` on the TypeScript side.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ModuleStatus {
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
    fn preview(&self, context: &ModuleContext) -> Result<PreviewReport, String>;

    /// Apply the module: install, configure, or launch as appropriate.
    /// Must record a `TransactionRecord` so the action is reversible.
    fn apply(&self, context: &ModuleContext) -> Result<ApplyResult, String>;

    /// Read-only state snapshot. Drives the verification checklist.
    /// Must not mutate state.
    fn verify(&self, context: &ModuleContext) -> Result<VerificationReport, String>;

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
    fn remove(&self, context: &ModuleContext) -> Result<ApplyResult, String>;

    /// Optional upstream-version check. The default returns
    /// `UpdateStatus::Unknown` so modules without a public release feed
    /// don't have to implement anything.
    fn update_check(&self, _context: &ModuleContext) -> Result<UpdateInfo, String> {
        Ok(UpdateInfo {
            status: UpdateStatus::Unknown,
            current_version: None,
            latest_version: None,
            release_url: None,
            detail: None,
        })
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
        fn preview(&self, _context: &ModuleContext) -> Result<PreviewReport, String> {
            Ok(PreviewReport {
                can_apply: true,
                changes: Vec::new(),
                warnings: Vec::new(),
            })
        }
        fn apply(&self, _context: &ModuleContext) -> Result<ApplyResult, String> {
            Err("apply not implemented in test stub".to_owned())
        }
        fn verify(&self, _context: &ModuleContext) -> Result<VerificationReport, String> {
            Ok(VerificationReport {
                status: ModuleStatus::Unknown,
                summary: "test stub".to_owned(),
                checks: Vec::new(),
                game_running: false,
                installed: false,
                installed_version: None,
            })
        }
        fn remove(&self, _context: &ModuleContext) -> Result<ApplyResult, String> {
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

    #[test]
    fn dummy_module_defaults_update_check_to_unknown() {
        let module = DummyModule;
        let context = ModuleContext {
            game_id: "test".to_owned(),
            game_name: "Test".to_owned(),
            install_dir: PathBuf::from("C:/Games/Test"),
            executable: "test.exe".to_owned(),
            work_dir: PathBuf::from("C:/Users/me/AppData/Local/Moddin/tools/dummy"),
        };
        let info = module.update_check(&context).expect("default update_check");
        assert_eq!(info.status, UpdateStatus::Unknown);
        assert!(info.latest_version.is_none());
    }
}