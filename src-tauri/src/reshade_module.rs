//! `Module` trait adapter for the ReShade host installer.
//!
//! This adapter is the **first** real adoption of [`crate::module::Module`]:
//! the catalog config travels through the `ReShadeRequest` that the
//! existing [`crate::reshade`] commands already accept, so we can build the
//! request from the [`ModuleContext`] + the catalog config payload
//! without waiting for a generic config-plumbing refactor.
//!
//! When the dispatcher hands us a context without a config the adapter
//! returns a stub preview / explicit "missing config" error instead of
//! silently applying a default recipe — the existing Tauri commands remain
//! the supported entry point in that mode.

use std::path::PathBuf;

use crate::{
    module::{
        ApplyResult, CheckCategory, CheckOutcome, CheckSeverity, Module, ModuleCategory,
        ModuleContext, ModuleStatus, PreviewReport, UpdateInfo, UpdateStatus,
        VerificationReport,
    },
    reshade::{self, ReshadeRequest},
    transaction::TransactionRecord,
};

pub struct ReshadeModule;

impl ReshadeModule {
    /// Build a [`ReshadeRequest`] from the [`ModuleContext`] and the
    /// merged catalog config. Requires the recipe to declare a `proxy`
    /// (the only field the request validation refuses to default).
    fn build_request(context: &ModuleContext, config: &serde_json::Value) -> Option<ReshadeRequest> {
        let proxy = config.get("proxy")?.as_str()?.to_owned();
        let version_policy = config
            .get("versionPolicy")
            .and_then(|v| v.as_str())
            .unwrap_or("pinned")
            .to_owned();
        let pinned_tag = config
            .get("pinnedTag")
            .and_then(|v| v.as_str())
            .map(str::to_owned);
        let archive_url = config
            .get("archiveUrl")
            .and_then(|v| v.as_str())
            .map(str::to_owned);
        let local_archive = config
            .get("localArchive")
            .and_then(|v| v.as_str())
            .map(str::to_owned);
        let sha256 = config
            .get("sha256")
            .and_then(|v| v.as_str())
            .map(str::to_owned);
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
        let update_url = config
            .get("updateUrl")
            .and_then(|v| v.as_str())
            .map(str::to_owned);

        Some(ReshadeRequest {
            game_id: context.game_id.clone(),
            game_name: context.game_name.clone(),
            install_dir: context.install_dir.to_string_lossy().into_owned(),
            executable: context.executable.clone(),
            version_policy,
            pinned_tag,
            archive_url,
            local_archive,
            sha256,
            proxy,
            safety_notes,
            update_url,
        })
    }
}

impl Module for ReshadeModule {
    fn id(&self) -> &'static str {
        "reshade"
    }

    fn name(&self) -> &'static str {
        "ReShade host"
    }

    fn category(&self) -> ModuleCategory {
        ModuleCategory::Graphics
    }

    async fn preview(&self, context: &ModuleContext) -> Result<PreviewReport, String> {
        let Some(config) = context.config.clone() else {
            return Ok(PreviewReport {
                can_apply: false,
                changes: vec![
                    "Forward to reshade::preview_reshade to see the planned changes.".to_owned(),
                ],
                warnings: vec![
                    "ReshadeModule needs the catalog recipe config (proxy, versionPolicy, \
                     archiveUrl, sha256) on ModuleContext.config. Use the existing \
                     reshade::preview_reshade Tauri command for now."
                        .to_owned(),
                ],
            });
        };

        let request = Self::build_request(context, &config)
            .ok_or_else(|| "ReshadeModule config is missing required fields.".to_owned())?;
        let preview = reshade::preview_reshade(request).await?;
        Ok(PreviewReport {
            can_apply: preview.can_apply,
            changes: preview.changes,
            warnings: preview.warnings,
        })
    }

    async fn apply(&self, context: &ModuleContext) -> Result<ApplyResult, String> {
        let config = context.config.clone().ok_or_else(|| {
            "ReshadeModule.apply needs the catalog recipe config on ModuleContext.config."
                .to_owned()
        })?;
        let request = Self::build_request(context, &config)
            .ok_or_else(|| "ReshadeModule config is missing required fields.".to_owned())?;
        let result = reshade::install_reshade(request).await?;
        Ok(ApplyResult {
            transaction: result.transaction.ok_or_else(|| {
                "ReShade install completed but did not return a transaction record.".to_owned()
            })?,
            installed: result.installed,
            started: false,
            armed: false,
            version: result.version,
            backend: Some(result.proxy),
        })
    }

    async fn verify(
        &self,
        context: &ModuleContext,
    ) -> Result<VerificationReport, String> {
        let Some(config) = context.config.clone() else {
            return Ok(VerificationReport {
                status: ModuleStatus::Unknown,
                summary: "ReShade: see reshade::preview_reshade for preconditions.".to_owned(),
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
                    summary: "ReShade recipe is missing required fields.".to_owned(),
                    checks: vec![CheckOutcome {
                        label: "Recipe config".to_owned(),
                        passed: false,
                        detail: Some("proxy/versionPolicy/archiveUrl/sha256 required".to_owned()),
                        ..Default::default()
                    }],
                    game_running: false,
                    installed: false,
                    installed_version: None,
                    definitions: Vec::new(),
                })
            }
        };

        let preview = reshade::preview_reshade(request).await?;
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
                "ReShade is installed ({}).",
                preview.installed_version.as_deref().unwrap_or("pinned")
            )
        } else if preview.can_apply {
            "ReShade is ready to install.".to_owned()
        } else {
            "ReShade cannot be applied yet; see checklist.".to_owned()
        };

        let mut checks = Vec::new();
        checks.push(CheckOutcome {
            id: Some("game-executable-present"),
            category: CheckCategory::Global,
            label: "Game executable present".to_owned(),
            passed: preview.executable_exists,
            detail: None,
            ..Default::default()
        });
        checks.push(CheckOutcome {
            id: Some("game-not-running"),
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
            id: Some("proxy-available"),
            category: CheckCategory::ModuleSpecific,
            severity: CheckSeverity::Blocker,
            label: format!("Proxy '{}' available", preview.proxy_chosen),
            passed: preview.proxy_available,
            detail: (!preview.proxy_available).then(|| {
                if preview.conflicts.is_empty() {
                    "Proxy DLL is already in place.".to_owned()
                } else {
                    preview
                        .conflicts
                        .iter()
                        .map(|c| format!("'{}' held by {}", c.proxy, c.held_by))
                        .collect::<Vec<_>>()
                        .join("; ")
                }
            }),
            ..Default::default()
        });
        checks.push(CheckOutcome {
            id: Some("archive-reachable"),
            category: CheckCategory::ModuleSpecific,
            severity: CheckSeverity::Warning,
            label: "Archive reachable".to_owned(),
            passed: preview.archive_reachable,
            detail: (!preview.archive_reachable)
                .then(|| "Set archiveUrl or localArchive in the recipe.".to_owned()),
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
        let transaction_id = None;
        let record: TransactionRecord =
            reshade::uninstall_reshade(context.game_id.clone(), transaction_id).await?;
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
                    "ReShade update_check needs ModuleContext.config.updateUrl.".to_owned(),
                ),
            });
        };
        let update_url = config.get("updateUrl").and_then(|v| v.as_str());
        let current_version = config
            .get("pinnedTag")
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
            work_dir: PathBuf::from("C:/Users/me/AppData/Local/Moddin/tools/reshade"),
            config,
        }
    }

    #[tokio::test]
    async fn reshade_module_reports_its_identity() {
        let module = ReshadeModule;
        assert_eq!(module.id(), "reshade");
        assert_eq!(module.name(), "ReShade host");
        assert_eq!(module.category(), ModuleCategory::Graphics);
    }

    #[tokio::test]
    async fn reshade_module_preview_without_config_returns_stub() {
        let module = ReshadeModule;
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
            "proxy": "version.dll",
            "versionPolicy": "pinned",
            "pinnedTag": "6.0.0",
            "archiveUrl": "https://github.com/crosire/reshade/releases/download/v6.0.0/ReShade.zip",
            "sha256": "0".repeat(64),
            "updateUrl": "https://api.github.com/repos/crosire/reshade/releases/latest",
            "safetyNotes": ["Note A", "Note B"]
        });
        let request = ReshadeModule::build_request(&context_with_config(Some(config.clone())), &config)
            .expect("config has all required fields");
        assert_eq!(request.proxy, "version.dll");
        assert_eq!(request.version_policy, "pinned");
        assert_eq!(request.pinned_tag.as_deref(), Some("6.0.0"));
        assert_eq!(request.sha256.as_deref(), Some("0".repeat(64).as_str()));
        assert_eq!(request.update_url.as_deref(), Some(
            "https://api.github.com/repos/crosire/reshade/releases/latest"
        ));
        assert_eq!(request.safety_notes.len(), 2);
    }

    #[test]
    fn build_request_requires_proxy_field() {
        let config = serde_json::json!({});
        assert!(ReshadeModule::build_request(
            &context_with_config(Some(config)),
            &serde_json::json!({})
        )
        .is_none());
    }
}

