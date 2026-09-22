//! Updater trait — extract of `Module::update_check` into a reusable
//! contract.
//!
//! Several modules already declare an `updateUrl` in their catalog
//! recipe (UEVR, OptiScaler, Cheeky, OFXR, ReShade) but the wiring sits
//! behind bespoke logic in each module's preview command. This trait
//! captures the shape in one place so:
//!
//! - a generic dispatcher can ask every registered `Updater` for the
//!   latest version and surface a single "updates available" panel;
//! - new modules can implement the trait instead of re-implementing
//!   the GitHub release-API dance that [`crate::updates`] already
//!   handles;
//! - tests can use [`Updater::latest`] without standing up a Tauri
//!   runtime.

use serde::{Deserialize, Serialize};

/// Status of a single update check, mirroring the TypeScript
/// `ModuleUpdateStatus` enum so the UI can render the result directly.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum UpdaterStatus {
    Unknown,
    Current,
    Available,
    Unavailable,
    Error,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdaterResult {
    pub status: UpdaterStatus,
    pub current_version: Option<String>,
    pub latest_version: Option<String>,
    pub release_url: Option<String>,
    pub detail: Option<String>,
}

pub trait Updater: Send + Sync {
    /// Stable catalog id (e.g. `"ofxr-framegen"`, `"uevr"`,
    /// `"optiscaler"`). Used by the generic dispatcher.
    fn id(&self) -> &'static str;

    /// GitHub release-API endpoint declared by the catalog recipe, or
    /// `None` when the module does not have an upstream feed.
    fn update_url(&self) -> Option<&str>;

    /// Perform the upstream check and return a structured result.
    async fn latest(&self, current_version: Option<&str>) -> Result<UpdaterResult, String>;
}

/// Adapts a catalog-declared `update_url` to the existing
/// [`crate::updates::check_module_update`] command. Modules can wrap a
/// static URL without standing up bespoke logic; the URL is still
/// validated by `updates::check_module_update` (HTTPS on github.com).
pub struct GitHubReleaseUpdater {
    pub module_id: &'static str,
    pub update_url: Option<String>,
}

impl GitHubReleaseUpdater {
    pub const fn new(module_id: &'static str, update_url: Option<String>) -> Self {
        Self {
            module_id,
            update_url,
        }
    }
}

impl Updater for GitHubReleaseUpdater {
    fn id(&self) -> &'static str {
        self.module_id
    }

    fn update_url(&self) -> Option<&str> {
        self.update_url.as_deref()
    }

    async fn latest(&self, current_version: Option<&str>) -> Result<UpdaterResult, String> {
        let result =
            crate::updates::check_module_update(crate::updates::ModuleUpdateRequest {
                current_version: current_version.map(str::to_owned),
                update_url: self.update_url.clone(),
            })
        .await?;

        let status = match result.status.as_str() {
            "current" => UpdaterStatus::Current,
            "available" => UpdaterStatus::Available,
            "unavailable" => UpdaterStatus::Unavailable,
            "error" => UpdaterStatus::Error,
            _ => UpdaterStatus::Unknown,
        };

        Ok(UpdaterResult {
            status,
            current_version: result.current_version,
            latest_version: result.latest_version,
            release_url: result.release_url,
            detail: result.detail,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_id_propagates() {
        let updater = GitHubReleaseUpdater::new("ofxr-framegen", None);
        assert_eq!(updater.id(), "ofxr-framegen");
        assert!(updater.update_url().is_none());
    }

    #[test]
    fn update_url_propagates() {
        let updater = GitHubReleaseUpdater::new(
            "uevr",
            Some("https://api.github.com/repos/praydog/UEVR-nightly/releases/latest".to_owned()),
        );
        assert_eq!(
            updater.update_url(),
            Some("https://api.github.com/repos/praydog/UEVR-nightly/releases/latest")
        );
    }
}