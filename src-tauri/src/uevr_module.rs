//! `Module` trait adapter for the UEVR engine-aware installer.
//!
//! Pilot adapter following the same pattern as [`crate::ofxr_module`].
//! Adopts the trait so future dispatchers can route UEVR through the
//! generic `Module` surface; the existing
//! `uevr::preview_uevr` / `uevr::install_uevr` / `uevr::uninstall_uevr`
//! Tauri commands keep their bespoke request structs until the catalog
//! config plumbing lands.

use std::path::PathBuf;

use crate::module::{
    ApplyResult, Module, ModuleCategory, ModuleContext, ModuleStatus, PreviewReport,
    UpdateInfo, UpdateStatus, VerificationReport,
};

pub struct UevrModule;

impl Module for UevrModule {
    fn id(&self) -> &'static str {
        "uevr"
    }

    fn name(&self) -> &'static str {
        "UEVR engine-aware installer"
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
                "Forward to uevr::preview_uevr to see the planned changes.".to_owned(),
            ],
            warnings: vec![
                "UevrModule is a pilot wrapper. The catalog config \
                 (versionPolicy, releaseApiUrl, afwReleaseApiUrl, backendReleaseApiUrl, \
                 defaultBackend, pinnedTag, backendCandidates) is not yet carried on \
                 ModuleContext.config, so the live preview command is unreachable from this \
                 adapter."
                    .to_owned(),
            ],
        })
    }

    async fn apply(
        &self,
        _context: &ModuleContext,
    ) -> Result<ApplyResult, String> {
        Err(
            "UevrModule.apply forwards to uevr::install_uevr which requires a fully-built \
             UevrRequest; the catalog config plumbing is pending."
                .to_owned(),
        )
    }

    async fn verify(
        &self,
        _context: &ModuleContext,
    ) -> Result<VerificationReport, String> {
        Ok(VerificationReport {
            status: ModuleStatus::Unknown,
            summary: "UEVR verification: see uevr::preview_uevr.can_apply".to_owned(),
            checks: Vec::new(),
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
        let record = crate::transaction::rollback_latest_module_transaction(
            context.game_id.clone(),
            "uevr".to_owned(),
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
        context: &ModuleContext,
    ) -> Result<UpdateInfo, String> {
        let Some(config) = context.config.clone() else {
            return Ok(UpdateInfo {
                status: UpdateStatus::Unknown,
                current_version: None,
                latest_version: None,
                release_url: None,
                detail: Some(
                    "UEVR update_check needs ModuleContext.config.releaseApiUrl.".to_owned(),
                ),
            });
        };
        // UEVR Nightly is the canonical release feed. The pilot reports
        // Unavailable so the UI knows the wiring is still pending — once
        // the catalog config plumbs through, this becomes a real lookup.
        let _ = config;
        Ok(UpdateInfo {
            status: UpdateStatus::Unavailable,
            current_version: None,
            latest_version: None,
            release_url: None,
            detail: Some(
                "UEVR release feed detection is tracked in ModuleContext.config.releaseApiUrl; \
                 the dispatcher wires this up in a follow-up PR."
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
            game_id: "dead-island-2".to_owned(),
            game_name: "Dead Island 2".to_owned(),
            install_dir: PathBuf::from("C:/Games/DeadIsland2"),
            executable: "DeadIsland/Binaries/Win64/DeadIsland-Win64-Shipping.exe".to_owned(),
            work_dir: PathBuf::from("C:/Users/me/AppData/Local/Moddin/tools/uevr"),
            config: None,
        }
    }

    #[tokio::test]
    async fn uevr_module_reports_its_identity() {
        let module = UevrModule;
        assert_eq!(module.id(), "uevr");
        assert_eq!(module.name(), "UEVR engine-aware installer");
        assert_eq!(module.category(), ModuleCategory::Vr);
    }

    #[tokio::test]
    async fn uevr_module_preview_without_config_returns_stub() {
        let module = UevrModule;
        let preview = module
            .preview(&context())
            .await
            .expect("pilot preview should succeed");
        assert!(!preview.can_apply);
        assert!(preview.warnings.iter().any(|w| w.contains("pilot")));
    }
}
