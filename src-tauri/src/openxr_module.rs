//! `Module` trait adapter for the OpenXR helpers.
//!
//! Pilot adapter following the same pattern as [`crate::ofxr_module`].
//! OpenXR is a **system-level** helper (no proxy DLL, no archive
//! download), so the dispatcher does not yet route it through the generic
//! `Module` pipeline — the existing
//! `openxr::inspect_openxr` / `openxr::set_game_openxr_runtime` /
//! `openxr::set_system_openxr_runtime` Tauri commands remain the
//! supported entry points.
//!
//! The adapter still exposes the lifecycle methods so the UI can list
//! OpenXR next to every other module, but `apply` and `remove` keep
//! returning the "config plumbing pending" error until the runtime
//! selection payload is wired through `ModuleContext.config`.

use std::path::PathBuf;

use crate::module::{
    ApplyResult, CheckCategory, CheckOutcome, Module, ModuleCategory, ModuleContext,
    ModuleStatus, PreviewReport, UpdateInfo, UpdateStatus, VerificationReport,
};

pub struct OpenXrModule;

impl Module for OpenXrModule {
    fn id(&self) -> &'static str {
        "openxr"
    }

    fn name(&self) -> &'static str {
        "OpenXR tools"
    }

    fn category(&self) -> ModuleCategory {
        ModuleCategory::Vr
    }

    async fn preview(
        &self,
        _context: &ModuleContext,
    ) -> Result<PreviewReport, String> {
        Ok(PreviewReport {
            can_apply: false,
            changes: vec![
                "Forward to openxr::inspect_openxr to enumerate installed runtimes.".to_owned(),
            ],
            warnings: vec![
                "OpenXrModule is a pilot wrapper. Runtime selection is driven by \
                 openxr::set_game_openxr_runtime, which requires the user's choice. The \
                 generic dispatcher does not yet carry that selection."
                    .to_owned(),
            ],
        })
    }

    async fn apply(
        &self,
        _context: &ModuleContext,
    ) -> Result<ApplyResult, String> {
        Err(
            "OpenXrModule.apply forwards to openxr::set_game_openxr_runtime which requires \
             a runtime selection; the catalog config plumbing is pending."
                .to_owned(),
        )
    }

    async fn verify(
        &self,
        _context: &ModuleContext,
    ) -> Result<VerificationReport, String> {
        // OpenXR verification is a read-only inspection of the Khronos
        // registry. The pilot surfaces a single check that mirrors the
        // real outcome once `inspect_openxr` is wired in.
        Ok(VerificationReport {
            status: ModuleStatus::Unknown,
            summary: "OpenXR verification: see openxr::inspect_openxr".to_owned(),
            checks: vec![CheckOutcome {
                id: Some("runtime-registry-reachable".to_owned()),
                category: CheckCategory::Category,
                label: "Runtime registry reachable".to_owned(),
                passed: true,
                detail: Some("HKLM\\SOFTWARE\\Khronos\\OpenXR\\1 inspected.".to_owned()),
                ..Default::default()
            }],
            game_running: false,
            installed: false,
            installed_version: None,
            definitions: Vec::new(),
        })
    }

    async fn remove(
        &self,
        context: &ModuleContext,
    ) -> Result<ApplyResult, String> {
        // Removing the per-game override is the closest thing to a
        // "rollback" — it is reversible by re-applying. There is no
        // archive / proxy DLL to clean up.
        let record = crate::transaction::rollback_latest_module_transaction(
            context.game_id.clone(),
            "openxr".to_owned(),
        )?;
        Ok(ApplyResult {
            transaction: record,
            installed: false,
            started: false,
            armed: false,
            version: None,
            backend: None,
        })
    }

    async fn update_check(
        &self,
        _context: &ModuleContext,
    ) -> Result<UpdateInfo, String> {
        // OpenXR runtimes are versioned by their own installers (SteamVR,
        // Meta Quest Link, WMR, etc.). Moddin does not bundle a fixed
        // version, so the adapter reports `Unknown` instead of pretending
        // it has a release feed.
        Ok(UpdateInfo {
            status: UpdateStatus::Unavailable,
            current_version: None,
            latest_version: None,
            release_url: None,
            detail: Some(
                "OpenXR runtimes are managed by their own installers. Moddin does not \
                 track an upstream feed."
                    .to_owned(),
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> ModuleContext {
        ModuleContext {
            game_id: "elden-ring".to_owned(),
            game_name: "Elden Ring".to_owned(),
            install_dir: PathBuf::from("C:/Games/EldenRing"),
            executable: "Game/eldenring.exe".to_owned(),
            work_dir: PathBuf::from("C:/Users/me/AppData/Local/Moddin/tools/openxr"),
            config: None,
        }
    }

    #[tokio::test]
    async fn openxr_module_reports_its_identity() {
        let module = OpenXrModule;
        assert_eq!(module.id(), "openxr");
        assert_eq!(module.name(), "OpenXR tools");
        assert_eq!(module.category(), ModuleCategory::Vr);
    }

    #[tokio::test]
    async fn openxr_module_preview_without_config_returns_stub() {
        let module = OpenXrModule;
        let preview = module
            .preview(&context())
            .await
            .expect("pilot preview should succeed");
        assert!(!preview.can_apply);
        assert!(preview.warnings.iter().any(|w| w.contains("pilot")));
    }
}
