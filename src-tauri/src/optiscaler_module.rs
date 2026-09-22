//! `Module` trait adapter for the OptiScaler installer.
//!
//! Pilot adapter following the same pattern as [`crate::ofxr_module`].
//! Adopts the trait so future dispatchers can route OptiScaler through
//! the generic `Module` surface; the existing
//! `optiscaler::preview_optiscaler` / `optiscaler::install_optiscaler` /
//! `optiscaler::uninstall_optiscaler` Tauri commands keep their bespoke
//! request structs until the catalog config plumbing lands.

use std::path::PathBuf;

use crate::module::{
    ApplyResult, Module, ModuleCategory, ModuleContext, ModuleStatus, PreviewReport,
    UpdateInfo, UpdateStatus, VerificationReport,
};

pub struct OptiScalerModule;

impl Module for OptiScalerModule {
    fn id(&self) -> &'static str {
        "optiscaler"
    }

    fn name(&self) -> &'static str {
        "OptiScaler"
    }

    fn category(&self) -> ModuleCategory {
        ModuleCategory::Graphics
    }

    async fn preview(
        &self,
        _context: &ModuleContext,
    ) -> Result<PreviewReport, String> {
        Ok(PreviewReport {
            can_apply: false,
            changes: vec![
                "Forward to optiscaler::preview_optiscaler to see the planned changes."
                    .to_owned(),
            ],
            warnings: vec![
                "OptiScalerModule is a pilot wrapper. The catalog config \
                 (version, downloadUrl, sha256, proxyCandidates) is not yet carried on \
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
            "OptiScalerModule.apply forwards to optiscaler::install_optiscaler which requires \
             a fully-built OptiScalerRequest; the catalog config plumbing is pending."
                .to_owned(),
        )
    }

    async fn verify(
        &self,
        _context: &ModuleContext,
    ) -> Result<VerificationReport, String> {
        Ok(VerificationReport {
            status: ModuleStatus::Unknown,
            summary: "OptiScaler verification: see optiscaler::preview_optiscaler.can_apply"
                .to_owned(),
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
            "optiscaler".to_owned(),
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
        // OptiScaler tracks upstream releases via the existing
        // updates::check_module_update Tauri command, keyed by the recipe's
        // updateUrl. The adapter delegates when the config is present.
        let Some(config) = context.config.clone() else {
            return Ok(UpdateInfo {
                status: UpdateStatus::Unknown,
                current_version: None,
                latest_version: None,
                release_url: None,
                detail: Some(
                    "OptiScaler update_check needs ModuleContext.config.updateUrl.".to_owned(),
                ),
            });
        };
        let request = crate::updates::ModuleUpdateRequest {
            current_version: config.get("version").and_then(|v| v.as_str()).map(str::to_owned),
            update_url: config.get("updateUrl").and_then(|v| v.as_str()).map(str::to_owned),
        };
        match crate::updates::check_module_update(request).await {
            Ok(result) => Ok(UpdateInfo {
                status: match result.status.as_str() {
                    "available" => UpdateStatus::Available,
                    "current" => UpdateStatus::Current,
                    "unavailable" => UpdateStatus::Unavailable,
                    "unknown" => UpdateStatus::Unknown,
                    _ => UpdateStatus::Error,
                },
                current_version: result.current_version,
                latest_version: result.latest_version,
                release_url: result.release_url,
                detail: result.detail,
            }),
            Err(error) => Ok(UpdateInfo {
                status: UpdateStatus::Error,
                current_version: None,
                latest_version: None,
                release_url: None,
                detail: Some(error),
            }),
        }
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
            work_dir: PathBuf::from("C:/Users/me/AppData/Local/Moddin/tools/optiscaler"),
            config: None,
        }
    }

    #[tokio::test]
    async fn optiscaler_module_reports_its_identity() {
        let module = OptiScalerModule;
        assert_eq!(module.id(), "optiscaler");
        assert_eq!(module.name(), "OptiScaler");
        assert_eq!(module.category(), ModuleCategory::Graphics);
    }

    #[tokio::test]
    async fn optiscaler_module_preview_without_config_returns_stub() {
        let module = OptiScalerModule;
        let preview = module
            .preview(&context())
            .await
            .expect("pilot preview should succeed");
        assert!(!preview.can_apply);
        assert!(preview.warnings.iter().any(|w| w.contains("pilot")));
    }
}
