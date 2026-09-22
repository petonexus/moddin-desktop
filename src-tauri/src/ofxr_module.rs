//! `Module` trait adapter for the OFXR FrameGen module.
//!
//! Mirrors [`crate::reshade_module::ReshadeModule`]: the catalog config
//! travels through `ModuleContext.config` and the adapter builds a
//! fully-populated [`crate::ofxr::OfxrRequest`] before delegating to
//! the existing [`crate::ofxr`] Tauri commands.
//!
//! When the dispatcher hands us a context without a config the adapter
//! returns a stub preview / explicit "missing config" error instead of
//! silently applying a default recipe — the existing Tauri commands
//! remain the supported entry point in that mode.

use std::path::PathBuf;

use crate::{
    module::{
        ApplyResult, CheckCategory, CheckOutcome, CheckSeverity, Module, ModuleCategory,
        ModuleContext, ModuleStatus, PreviewReport, UpdateInfo, UpdateStatus,
        VerificationReport,
    },
    ofxr::{self, OfxrRequest},
    transaction::TransactionRecord,
};

pub struct OfxrModule;

impl OfxrModule {
    /// Build an [`OfxrRequest`] from the [`ModuleContext`] and the
    /// merged catalog config. Required fields: `version`,
    /// `implementationVersion`, `downloadUrl`, `sha256`, `backend`,
    /// `nvidiaPreset`, `nvidiaInputScale`.
    fn build_request(context: &ModuleContext, config: &serde_json::Value) -> Option<OfxrRequest> {
        let version = config.get("version")?.as_str()?.to_owned();
        let implementation_version = config
            .get("implementationVersion")
            .and_then(|v| v.as_u64())
            .map(|value| value as u32)?;
        let download_url = config.get("downloadUrl")?.as_str()?.to_owned();
        let sha256 = config.get("sha256")?.as_str()?.to_owned();
        let backend = config.get("backend")?.as_str()?.to_owned();
        let nvidia_preset = config
            .get("nvidiaPreset")
            .and_then(|v| v.as_str())
            .unwrap_or("medium")
            .to_owned();
        let nvidia_input_scale = config
            .get("nvidiaInputScale")
            .and_then(|v| v.as_u64())
            .map(|value| value as u32)
            .unwrap_or(50);
        let nvidia_bidirectional = config
            .get("nvidiaBidirectional")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let force_reinstall = config
            .get("forceReinstall")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let safety_notes = config
            .get("safetyNotes")
            .and_then(|v| v.as_array())
            .map(|array| {
                array
                    .iter()
                    .filter_map(|entry| entry.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();

        Some(OfxrRequest {
            game_id: context.game_id.clone(),
            game_name: context.game_name.clone(),
            install_dir: context.install_dir.to_string_lossy().into_owned(),
            executable: context.executable.clone(),
            version,
            implementation_version,
            download_url,
            sha256,
            backend,
            nvidia_preset,
            nvidia_input_scale,
            nvidia_bidirectional,
            force_reinstall,
            safety_notes,
        })
    }
}

impl Module for OfxrModule {
    fn id(&self) -> &'static str {
        "ofxr-framegen"
    }

    fn name(&self) -> &'static str {
        "OFXR FrameGen"
    }

    fn category(&self) -> ModuleCategory {
        ModuleCategory::Vr
    }

    async fn preview(&self, context: &ModuleContext) -> Result<PreviewReport, String> {
        let Some(config) = context.config.clone() else {
            return Ok(PreviewReport {
                can_apply: false,
                changes: vec![
                    "Forward to ofxr::preview_ofxr to see the planned changes.".to_owned(),
                ],
                warnings: vec![
                    "OfxrModule needs the catalog recipe config (version, implementationVersion, \
                     downloadUrl, sha256, backend, nvidiaPreset, nvidiaInputScale) on \
                     ModuleContext.config. Use the existing ofxr::preview_ofxr Tauri command for now."
                        .to_owned(),
                ],
            });
        };

        let request = Self::build_request(context, &config)
            .ok_or_else(|| "OfxrModule config is missing required fields.".to_owned())?;
        let preview = ofxr::preview_ofxr(request).await?;
        Ok(PreviewReport {
            can_apply: preview.can_apply,
            changes: preview.changes,
            warnings: preview.warnings,
        })
    }

    async fn apply(&self, context: &ModuleContext) -> Result<ApplyResult, String> {
        let config = context.config.clone().ok_or_else(|| {
            "OfxrModule.apply needs the catalog recipe config on ModuleContext.config.".to_owned()
        })?;
        let request = Self::build_request(context, &config)
            .ok_or_else(|| "OfxrModule config is missing required fields.".to_owned())?;
        let result = ofxr::install_ofxr(request).await?;
        Ok(ApplyResult {
            transaction: result.transaction.ok_or_else(|| {
                "OFXR install completed but did not return a transaction record.".to_owned()
            })?,
            installed: result.installed,
            started: result.started,
            armed: result.armed,
            version: Some(result.version),
            backend: Some(result.backend),
        })
    }

    async fn verify(
        &self,
        context: &ModuleContext,
    ) -> Result<VerificationReport, String> {
        let Some(config) = context.config.clone() else {
            return Ok(VerificationReport {
                status: ModuleStatus::Unknown,
                summary: "OFXR: see ofxr::preview_ofxr for preconditions.".to_owned(),
                checks: Vec::new(),
                game_running: false,
                installed: false,
                installed_version: None,
            definitions: Vec::new(),
            });
        };

        let request = match Self::build_request(context, &config) {
            Some(request) => request,
            None => {
                return Ok(VerificationReport {
                    status: ModuleStatus::Attention,
                    summary: "OFXR recipe is missing required fields.".to_owned(),
                    checks: vec![CheckOutcome {
                        label: "Recipe config".to_owned(),
                        passed: false,
                        detail: Some(
                            "version/implementationVersion/downloadUrl/sha256/backend/nvidiaPreset/nvidiaInputScale required"
                                .to_owned(),
                        ),
                        ..Default::default()
                    }],
                    game_running: false,
                    installed: false,
                    installed_version: None,
                    definitions: Vec::new(),
                })
            }
        };

        let preview = ofxr::preview_ofxr(request).await?;
        let status = if preview.game_running {
            ModuleStatus::Attention
        } else if preview.installed {
            ModuleStatus::Installed
        } else if preview.can_apply {
            ModuleStatus::Ready
        } else {
            ModuleStatus::Attention
        };

        let summary = if preview.installed {
            format!(
                "OFXR Bridge is installed ({}).",
                preview.installed_version.as_deref().unwrap_or("pinned")
            )
        } else if preview.can_apply {
            "OFXR Bridge is ready to install.".to_owned()
        } else {
            "OFXR Bridge cannot be applied yet; see checklist.".to_owned()
        };

        let mut checks = Vec::new();
        checks.push(CheckOutcome {
            id: Some("game-executable-present".to_owned()),
            category: CheckCategory::Global,
            label: "Game executable present".to_owned(),
            passed: preview.executable_exists,
            detail: None,
            ..Default::default()
        });
        checks.push(CheckOutcome {
            id: Some("game-not-running".to_owned()),
            category: CheckCategory::Global,
            severity: CheckSeverity::Blocker,
            label: "Game not running".to_owned(),
            passed: !preview.game_running,
            detail: preview
                .game_running
                .then(|| "Close the game before applying.".to_owned()),
            ..Default::default()
        });
        checks.push(CheckOutcome {
            id: Some("tray-path-present".to_owned()),
            category: CheckCategory::ModuleSpecific,
            label: format!("Tray path present ({})", preview.tray_path),
            passed: preview.tray_installed,
            detail: (!preview.tray_installed)
                .then(|| "Tray executable was not found at the expected path.".to_owned()),
            ..Default::default()
        });
        checks.push(CheckOutcome {
            id: Some("tray-configured".to_owned()),
            category: CheckCategory::ModuleSpecific,
            label: "Tray configured".to_owned(),
            passed: preview.configured,
            detail: (!preview.configured)
                .then(|| "OFXR tray.ini does not match the recommended preset.".to_owned()),
            ..Default::default()
        });
        checks.push(CheckOutcome {
            id: Some("tray-running".to_owned()),
            category: CheckCategory::ModuleSpecific,
            severity: CheckSeverity::Warning,
            label: "Tray running".to_owned(),
            passed: preview.tray_running,
            detail: (!preview.tray_running)
                .then(|| "OFXRBridgeTray.exe is not running; start it from the system tray.".to_owned()),
            ..Default::default()
        });

        Ok(VerificationReport {
            status,
            summary,
            checks,
            game_running: preview.game_running,
            installed: preview.installed,
            installed_version: preview.installed_version,
            definitions: Vec::new(),
        })
    }

    async fn remove(&self, context: &ModuleContext) -> Result<ApplyResult, String> {
        let config = context
            .config
            .clone()
            .ok_or_else(|| "OfxrModule.remove needs the catalog recipe config.".to_owned())?;
        let request = Self::build_request(context, &config)
            .ok_or_else(|| "OfxrModule.remove config is missing required fields.".to_owned())?;
        let record = ofxr::uninstall_ofxr(request)?;
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
                    "OFXR update_check needs ModuleContext.config.updateUrl.".to_owned(),
                ),
            });
        };
        let update_url = config.get("updateUrl").and_then(|v| v.as_str());
        let current_version = config
            .get("version")
            .and_then(|v| v.as_str())
            .map(str::to_owned);
        let request = crate::updates::ModuleUpdateRequest {
            current_version: current_version.clone(),
            update_url: update_url.map(str::to_owned),
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
                current_version: result.current_version.or(current_version),
                latest_version: result.latest_version,
                release_url: result.release_url,
                detail: result.detail,
            }),
            Err(error) => Ok(UpdateInfo {
                status: UpdateStatus::Error,
                current_version,
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

    fn context_with_config(config: Option<serde_json::Value>) -> ModuleContext {
        ModuleContext {
            game_id: "elden-ring".to_owned(),
            game_name: "Elden Ring".to_owned(),
            install_dir: PathBuf::from("C:/Games/EldenRing"),
            executable: "Game/eldenring.exe".to_owned(),
            work_dir: PathBuf::from("C:/Users/me/AppData/Local/Moddin/tools/ofxr"),
            config,
        }
    }

    #[tokio::test]
    async fn ofxr_module_reports_its_identity() {
        let module = OfxrModule;
        assert_eq!(module.id(), "ofxr-framegen");
        assert_eq!(module.name(), "OFXR FrameGen");
        assert_eq!(module.category(), ModuleCategory::Vr);
    }

    #[tokio::test]
    async fn ofxr_module_preview_without_config_returns_stub() {
        let module = OfxrModule;
        let preview = module
            .preview(&context_with_config(None))
            .await
            .expect("stub preview should succeed");
        assert!(!preview.can_apply);
        assert!(preview.warnings.iter().any(|w| w.contains("ModuleContext.config")));
    }

    #[test]
    fn build_request_pulls_every_field_from_config() {
        let config = serde_json::json!({
            "version": "0.2.0",
            "implementationVersion": 68,
            "downloadUrl": "https://github.com/tig3rmast3r/OFXR-Bridge/releases/download/0.2.0/OFXR.zip",
            "sha256": "3a4db67abd3d7fd013c9ef6878d8b7e23d312e84fe9c2c39ae509e4e554cb7f4",
            "backend": "fidelityfx",
            "nvidiaPreset": "medium",
            "nvidiaInputScale": 50,
            "nvidiaBidirectional": false,
            "forceReinstall": false,
            "safetyNotes": ["experimental"]
        });
        let request = OfxrModule::build_request(&context_with_config(Some(config.clone())), &config)
            .expect("complete config should build");
        assert_eq!(request.version, "0.2.0");
        assert_eq!(request.implementation_version, 68);
        assert_eq!(request.download_url.starts_with("https://github.com/"), true);
        assert_eq!(request.backend, "fidelityfx");
        assert_eq!(request.nvidia_preset, "medium");
        assert_eq!(request.nvidia_input_scale, 50);
        assert!(!request.nvidia_bidirectional);
        assert_eq!(request.safety_notes, vec!["experimental".to_owned()]);
    }

    #[test]
    fn build_request_rejects_missing_required_field() {
        let config = serde_json::json!({
            "version": "0.2.0"
            // implementationVersion, downloadUrl, sha256, backend missing
        });
        let request = OfxrModule::build_request(&context_with_config(Some(config)), &serde_json::json!({}));
        assert!(request.is_none());
    }
}