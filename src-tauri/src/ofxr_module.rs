//! Pilot `Module` adapter for the OFXR FrameGen module.
//!
//! Demonstrates the incremental adoption pattern for [`crate::module::Module`]:
//! the existing `ofxr::preview_ofxr`, `ofxr::install_ofxr`, and
//! `ofxr::uninstall_ofxr` Tauri commands keep their bespoke request
//! structs and validation. This adapter wraps them so the trait
//! signature is exercised end-to-end, with no risk of regressing the
//! production install path.
//!
//! Adoption note: when the catalog supplies the OFXR config inline (a
//! future `ModuleContext.config` field), this wrapper will build a
//! fully-populated `OfxrRequest` and forward to the real commands.
//! Until then, `preview` surfaces a stub so the UI can still drive the
//! flow via the existing `ofxr::*` commands.

use crate::{
    module::{Module, ModuleCategory, ModuleContext, PreviewReport, UpdateInfo, UpdateStatus},
    ofxr::{self, OfxrRequest},
};

pub struct OfxrModule;

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

    async fn preview(&self, _context: &ModuleContext) -> Result<PreviewReport, String> {
        // Pilot only: a real delegation would call `ofxr::preview_ofxr`
        // with a fully-built request (version, downloadUrl, sha256,
        // backend, etc.). Until the catalog config flows through
        // `ModuleContext`, surface a stub so the UI keeps using the
        // existing `ofxr::*` Tauri commands directly.
        Ok(PreviewReport {
            can_apply: false,
            changes: vec![
                "Forward to ofxr::preview_ofxr to see the planned changes.".to_owned(),
            ],
            warnings: vec![
                "OfxrModule is a pilot wrapper. The catalog config (version, \
                 implementationVersion, downloadUrl, sha256, backend, etc.) is \
                 not yet carried on ModuleContext, so the live preview command is \
                 unreachable from this adapter."
                    .to_owned(),
            ],
        })
    }

    async fn apply(
        &self,
        context: &ModuleContext,
    ) -> Result<crate::module::ApplyResult, String> {
        let Some(request) = build_request(context) else {
            return Err(
                "OfxrModule pilot cannot install yet: ModuleContext does not carry \
                 the OFXR config. Use ofxr::install_ofxr directly."
                    .to_owned(),
            );
        };
        let result = ofxr::install_ofxr(request).await?;
        Ok(crate::module::ApplyResult {
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
        _context: &ModuleContext,
    ) -> Result<crate::module::VerificationReport, String> {
        // Reuse the preview surface for v1: the verification checklist UI
        // already understands the OFXR preview fields.
        Ok(crate::module::VerificationReport {
            status: crate::module::ModuleStatus::Unknown,
            summary: "OFXR verification: see ofxr::preview_ofxr.can_apply".to_owned(),
            checks: Vec::new(),
            game_running: false,
            installed: false,
            installed_version: None,
        })
    }

    async fn remove(
        &self,
        _context: &ModuleContext,
    ) -> Result<crate::module::ApplyResult, String> {
        // Forwards to ofxr::uninstall_ofxr with a minimal request derived
        // from the catalog context. The transaction store handles the
        // rollback uniformly across all modules.
        Err(
            "OfxrModule.remove forwards to ofxr::uninstall_ofxr which requires \
             a fully-built OfxrRequest; the catalog config plumbing is pending."
                .to_owned(),
        )
    }

    async fn update_check(
        &self,
        _context: &ModuleContext,
    ) -> Result<UpdateInfo, String> {
        // OFXR tracks upstream releases via the existing
        // `updates::check_module_update` Tauri command, keyed by the
        // recipe's `updateUrl`. The wrapper leaves the surface intact
        // until the catalog config flows through ModuleContext.
        Ok(UpdateInfo {
            status: UpdateStatus::Unknown,
            current_version: None,
            latest_version: None,
            release_url: None,
            detail: Some(
                "OFXR update_check is served by updates::check_module_update; \
                 the catalog config plumbing is pending in ModuleContext."
                    .to_owned(),
            ),
        })
    }
}

fn build_request(context: &ModuleContext) -> Option<OfxrRequest> {
    if context.game_id.is_empty() || context.install_dir.as_os_str().is_empty() {
        return None;
    }
    Some(OfxrRequest {
        game_id: context.game_id.clone(),
        game_name: context.game_name.clone(),
        install_dir: context.install_dir.to_string_lossy().into_owned(),
        executable: context.executable.clone(),
        version: String::new(),
        implementation_version: 0,
        download_url: String::new(),
        sha256: String::new(),
        backend: String::new(),
        nvidia_preset: String::new(),
        nvidia_input_scale: 0,
        nvidia_bidirectional: false,
        force_reinstall: false,
        safety_notes: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn context() -> ModuleContext {
        ModuleContext {
            game_id: "test".to_owned(),
            game_name: "Test".to_owned(),
            install_dir: PathBuf::from("C:/Games/Test"),
            executable: "test.exe".to_owned(),
            work_dir: PathBuf::from("C:/Users/me/AppData/Local/Moddin/tools/ofxr"),
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
        // A real `ofxr::preview_ofxr` call needs a fully-built request;
        // the pilot must short-circuit before the missing fields fail
        // validation.
        let preview = module
            .preview(&context())
            .await
            .expect("pilot preview should succeed");
        assert!(!preview.can_apply);
        assert!(!preview.warnings.is_empty());
    }
}