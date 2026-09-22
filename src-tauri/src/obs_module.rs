//! `Module` trait adapter for the OBS VR capture module.
//!
//! Pilot adapter that follows the same pattern as [`crate::ofxr_module`]:
//! the existing Tauri commands keep their bespoke request structs and
//! validation. The trait adapter is exercised end-to-end with a stub
//! preview until the catalog config flows through `ModuleContext.config`.
//!
//! When the catalog config lands on `ModuleContext`, `build_request` will
//! populate the `ObsVrRequest` fields and the wrapper will forward to
//! the existing `obs::preview_obs_vr` / `obs::configure_obs_vr` /
//! `obs::uninstall_obs_vr` commands.

use std::path::PathBuf;

use crate::module::{
    ApplyResult, Module, ModuleCategory, ModuleContext, PreviewReport, UpdateInfo, UpdateStatus,
    VerificationReport,
};

pub struct ObsVrModule;

impl Module for ObsVrModule {
    fn id(&self) -> &'static str {
        "obs-vr"
    }

    fn name(&self) -> &'static str {
        "OBS VR Capture"
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
                "Forward to obs::preview_obs_vr to see the planned changes.".to_owned(),
            ],
            warnings: vec![
                "ObsVrModule is a pilot wrapper. The catalog config \
                 (collectionName, sceneName, sourceName, executableName) is not yet \
                 carried on ModuleContext.config, so the live preview command is \
                 unreachable from this adapter."
                    .to_owned(),
            ],
        })
    }

    async fn apply(
        &self,
        _context: &ModuleContext,
    ) -> Result<ApplyResult, String> {
        Err(
            "ObsVrModule.apply forwards to obs::configure_obs_vr which requires a fully-built \
             ObsVrRequest; the catalog config plumbing is pending."
                .to_owned(),
        )
    }

    async fn verify(
        &self,
        _context: &ModuleContext,
    ) -> Result<VerificationReport, String> {
        Ok(VerificationReport {
            status: crate::module::ModuleStatus::Unknown,
            summary: "OBS VR verification: see obs::preview_obs_vr.can_apply".to_owned(),
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
        // Removal rolls back the latest OBS VR transaction for the game
        // via the transaction store, which is uniform across modules.
        let record =
            crate::transaction::rollback_latest_module_transaction(context.game_id.clone(), "obs-vr".to_owned())?;
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
        Ok(UpdateInfo {
            status: UpdateStatus::Unknown,
            current_version: None,
            latest_version: None,
            release_url: None,
            detail: Some(
                "OBS VR does not have a fixed upstream release; updates are managed via \
                 the user's OBS install."
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
            work_dir: PathBuf::from("C:/Users/me/AppData/Local/Moddin/tools/obs-vr"),
            config: None,
        }
    }

    #[tokio::test]
    async fn obs_vr_module_reports_its_identity() {
        let module = ObsVrModule;
        assert_eq!(module.id(), "obs-vr");
        assert_eq!(module.name(), "OBS VR Capture");
        assert_eq!(module.category(), ModuleCategory::Vr);
    }

    #[tokio::test]
    async fn obs_vr_module_preview_without_config_returns_stub() {
        let module = ObsVrModule;
        let preview = module
            .preview(&context())
            .await
            .expect("pilot preview should succeed");
        assert!(!preview.can_apply);
        assert!(preview.warnings.iter().any(|w| w.contains("pilot")));
    }
}
