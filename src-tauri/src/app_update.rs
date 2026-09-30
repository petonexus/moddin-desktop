//! App self-updater — ROADMAP `F-04` / `P0-4`.
//!
//! Four properties this module is built around, each of them a
//! consequence of what the audit recorded the last time the updater was
//! needed: the signing key was a Rust constant, rotating it meant shipping
//! a binary, and meanwhile *"users downloaded a knowingly broken
//! installer"*.
//!
//! **The key is configuration, and a placeholder is not a key.**
//! `tauri-plugin-updater` reads `plugins.updater.pubkey` from
//! `tauri.conf.json`; the slot currently holds a marked placeholder, and
//! three separate refusals keep it from behaving like a real one:
//!
//! 1. a `const` assertion below fails the build in release — the profile
//!    that can produce an installer cannot compile against the marker;
//! 2. [`app_update_check`] and [`app_update_install`] return
//!    [`CheckStatus::NotConfigured`] instead of contacting the endpoint;
//! 3. there is no fallback path anywhere in this file. An unusable key is
//!    a refusal, never "download it anyway".
//!
//! **The digest is not ours.** `release.yml` builds the installer and
//! appends a SHA-256 computed from that artifact; this module never
//! compares, stores or recomputes one. The artifact's authenticity is the
//! minisign signature the plugin verifies against the pubkey, and the only
//! place that key exists is the config file the maintainer edits.
//!
//! **Opt-in.** Nothing in this file runs on a timer. The check happens
//! when the user asks for it from the sidebar panel
//! (`src/features/app-update/`), the download happens after they confirm a
//! version they read, and a version they decline is not offered again —
//! see [`resolve`], which is the only place an offer is created.
//!
//! **A failed update leaves the app running.** Three failures, three
//! answers, and none of them is "carry on without it":
//!
//! - the manifest is unreachable or names no artifact for this platform →
//!   nothing was written, [`FailureKind::Manifest`];
//! - the download fails, or its signature does not verify against the
//!   configured pubkey → nothing is installed, [`FailureKind::Signature`]
//!   or [`FailureKind::Download`];
//! - the installer cannot be launched → the plugin returns before it
//!   touches the installed files, [`FailureKind::Install`].
//!
//! On Windows a *successful* install is the one path that ends the
//! process: the plugin launches the NSIS installer and exits, and the
//! installer restarts Moddin itself (`/R`, see
//! `tauri_plugin_updater::updater::Update::install_inner`). That is why
//! there is no relaunch command here and no `tauri-plugin-process`
//! dependency: on the platform this app ships, the relaunch is the
//! installer's, and a second path for it would be a path with no caller.

use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::{Error as UpdaterError, UpdaterExt};

use crate::updates::{compare_versions, parse_version};

/// The marker in `plugins.updater.pubkey` that says no key is
/// substituted yet. The whole string is longer than this and explains the
/// substitution; this is the part the build and the runtime look for.
const PUBKEY_PLACEHOLDER: &str = "REPLACE_WITH_TAURI_UPDATER_PUBKEY";

/// The config as written, so the placeholder can be a compile error in a
/// release build rather than something discovered by a user. The file is
/// already compiled into the binary by `generate_context!`.
const TAURI_CONFIG: &str = include_str!("../tauri.conf.json");

/// Download progress, emitted to the webview while an install runs.
const PROGRESS_EVENT: &str = "app-update://progress";

/// The only release channel this app offers. Named here so the payload
/// says which one answered rather than leaving the UI to assume.
const CHANNEL: &str = "stable";

/// Byte search usable in a `const` context.
const fn contains(haystack: &str, needle: &str) -> bool {
    let bytes = haystack.as_bytes();
    let target = needle.as_bytes();
    if target.is_empty() || target.len() > bytes.len() {
        return false;
    }

    let mut start = 0;
    while start + target.len() <= bytes.len() {
        let mut offset = 0;
        while offset < target.len() && bytes[start + offset] == target[offset] {
            offset += 1;
        }
        if offset == target.len() {
            return true;
        }
        start += 1;
    }

    false
}

/// Whether `plugins.updater.pubkey` is still the placeholder.
pub(crate) const fn placeholder_in_place(config: &str) -> bool {
    contains(config, PUBKEY_PLACEHOLDER)
}

/// A release build cannot be produced against the placeholder.
///
/// `not(debug_assertions)` is deliberate and load-bearing: this guard is
/// for `tauri build`, the only thing that can ship an installer, and the
/// repository is currently *in* the placeholder state on purpose. Leaving
/// it on in debug builds would make `cargo test` and `cargo clippy` — the
/// two gates that are now blocking — permanently red, which is how a
/// guard gets disabled.
#[cfg(not(debug_assertions))]
const _: () = assert!(
    !placeholder_in_place(TAURI_CONFIG),
    "tauri.conf.json still holds the updater pubkey placeholder. Run `npm run tauri signer generate`, paste the printed public key into plugins.updater.pubkey, and read docs/UPDATER.md before building a release."
);

/// Why an announced release is not offered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// The version carries a prerelease suffix (`0.2.0-rc.1`). The stable
    /// channel is a rule here, not a property of the endpoint: a
    /// misconfigured or replaced manifest must not be able to talk a
    /// stable install into a beta.
    Prerelease,
    /// A version string this build cannot compare. An unparseable version
    /// is a refusal, never "assume it is newer".
    UnreadableVersion,
}

/// The decision [`decide`] makes about one announced version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Offer {
    Available { version: String },
    UpToDate,
    Refused(Refusal),
}

/// Version comparison plus the stable-channel rule.
///
/// Comparison itself is [`crate::updates`]' three-way compare, the same
/// one the `exe-version` check uses, so "is this newer" has one answer
/// in the app.
pub(crate) fn decide(current_version: &str, announced_version: &str) -> Offer {
    let (Some(current), Some(announced)) = (
        parse_version(current_version),
        parse_version(announced_version),
    ) else {
        return Offer::Refused(Refusal::UnreadableVersion);
    };

    if announced.prerelease.is_some() {
        return Offer::Refused(Refusal::Prerelease);
    }

    if compare_versions(&announced, &current) == Ordering::Greater {
        Offer::Available {
            version: announced_version.trim().to_owned(),
        }
    } else {
        Offer::UpToDate
    }
}

/// What a check reports. This is the only way a release becomes an offer,
/// so it is the only place the stable-channel rule and "already declined"
/// can be enforced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Resolution {
    Available {
        version: String,
    },
    /// The user turned this version down. Say nothing about it: the panel
    /// shows the current version and the check button, and nothing else.
    Declined {
        version: String,
    },
    UpToDate,
    Unavailable {
        reason: Refusal,
    },
}

/// Combine version comparison with the user's earlier answer.
///
/// `announced_version` is `None` when the endpoint had nothing newer, so
/// a release that never existed is the same answer as one already
/// installed. `declined_version` is what the user last turned down; it is
/// carried by the panel rather than read from disk here, because there is
/// no background task that could act on it without the user asking.
pub(crate) fn resolve(
    current_version: &str,
    announced_version: Option<&str>,
    declined_version: Option<&str>,
) -> Resolution {
    let Some(announced) = announced_version
        .map(str::trim)
        .filter(|version| !version.is_empty())
    else {
        return Resolution::UpToDate;
    };

    match decide(current_version, announced) {
        Offer::UpToDate => Resolution::UpToDate,
        Offer::Refused(reason) => Resolution::Unavailable { reason },
        Offer::Available { version } => {
            if declined_version.map(str::trim) == Some(version.as_str()) {
                Resolution::Declined { version }
            } else {
                Resolution::Available { version }
            }
        }
    }
}

/// How an update attempt ended. Every kind is a refusal to continue: the
/// installed version is what the user keeps in all of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    /// The pubkey is still the placeholder, so nothing can be verified.
    NotConfigured,
    /// The artifact's signature did not verify against the configured
    /// pubkey. The one failure that must never have a fallback.
    Signature,
    /// The artifact could not be fetched.
    Download,
    /// The endpoint answered, but not with an artifact for this platform.
    Manifest,
    /// The verified artifact could not be handed to an installer.
    Install,
    /// The release moved between the check and the confirmation. Nothing
    /// is installed, because the user confirmed a different version.
    Changed,
    /// Anything the plugin reports that this build does not recognise.
    Unknown,
}

impl FailureKind {
    /// The message the frontend classifies. It names the failure class in
    /// words because that is what `useFriendlyError` matches on, and it is
    /// kept next to the classification so the two cannot drift.
    pub fn message(self) -> &'static str {
        match self {
            Self::NotConfigured => {
                "This build has no update signing key, so Moddin cannot verify an update. Nothing was downloaded."
            }
            Self::Signature => {
                "The downloaded update is not signed by Moddin's release key, so it was refused. Nothing was installed."
            }
            Self::Download => {
                "The update could not be downloaded. Nothing was installed, and this version of Moddin still works."
            }
            Self::Manifest => {
                "The update feed did not offer an installer for this PC. Nothing was installed."
            }
            Self::Install => {
                "The update could not be installed. The version of Moddin you have now was left untouched."
            }
            Self::Changed => {
                "The release changed since it was offered, so nothing was installed. Check for updates again to see the new version."
            }
            Self::Unknown => {
                "The update could not be completed. Nothing was installed, and this version of Moddin still works."
            }
        }
    }

    /// Map a plugin failure onto one of the kinds above.
    ///
    /// Matched on the error variant, not on its text, so a wording change
    /// upstream cannot quietly reclassify a signature failure as a
    /// network hiccup. `#[non_exhaustive]`, hence the arm that says so.
    pub fn from_updater_error(error: &UpdaterError) -> Self {
        match error {
            // `Minisign` is the variant a real bad signature arrives as;
            // its siblings are the same refusal with the failure surfacing
            // earlier, while the signature is being read.
            UpdaterError::Minisign(_)
            | UpdaterError::SignatureUtf8(_)
            | UpdaterError::Base64(_) => Self::Signature,
            UpdaterError::Network(_) | UpdaterError::Reqwest(_) | UpdaterError::Http(_) => {
                Self::Download
            }
            UpdaterError::ReleaseNotFound
            | UpdaterError::TargetNotFound(_)
            | UpdaterError::TargetsNotFound(_)
            | UpdaterError::UnsupportedArch
            | UpdaterError::UnsupportedOs
            | UpdaterError::EmptyEndpoints
            | UpdaterError::InsecureTransportProtocol => Self::Manifest,
            UpdaterError::PackageInstallFailed
            | UpdaterError::DebInstallFailed
            | UpdaterError::Io(_)
            | UpdaterError::Tauri(_)
            | UpdaterError::TempDirNotFound
            | UpdaterError::BinaryNotFoundInArchive
            | UpdaterError::InvalidUpdaterFormat
            | UpdaterError::TempDirNotOnSameMountPoint
            | UpdaterError::FailedToDetermineExtractPath => Self::Install,
            _ => Self::Unknown,
        }
    }
}

/// What `app_update_status` reports. Read once when the panel opens.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateStatus {
    /// Whether a real pubkey is in place. `false` means the updater is
    /// wired but cannot be trusted, and the panel says so rather than
    /// offering a check that can only fail.
    pub configured: bool,
    pub current_version: String,
    /// Always `stable`; the panel prints it so the channel is never
    /// implied.
    pub channel: &'static str,
    /// Present only when `configured` is false.
    pub detail: Option<String>,
}

/// Mirrors the frontend's `AppUpdateCheckRequest`.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateCheckRequest {
    /// The last version the user turned down, if any.
    pub declined_version: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum CheckStatus {
    /// The endpoint has nothing newer, or nothing at all.
    UpToDate,
    /// An update the user has not seen yet.
    Available,
    /// An update the user already turned down. Not an error, and not
    /// something to prompt about again.
    Declined,
    /// The endpoint answered with something this build will not offer —
    /// a prerelease, or a version it cannot read.
    Unavailable,
    /// The pubkey is still the placeholder.
    NotConfigured,
}

/// Mirrors the frontend's `AppUpdateCheck`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateCheck {
    pub status: CheckStatus,
    pub current_version: String,
    /// The version the status refers to: the offered one, or the declined
    /// one so the panel can name what it is not offering.
    pub version: Option<String>,
    pub notes: Option<String>,
    pub published_at: Option<String>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateProgress {
    /// `downloading` while bytes arrive, `installing` once the artifact is
    /// verified and handed over.
    pub phase: &'static str,
    pub downloaded: u64,
    pub total: Option<u64>,
}

/// Mirrors the frontend's `AppUpdateInstallRequest`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateInstallRequest {
    /// The version the user confirmed. Install refuses a release that no
    /// longer matches it.
    pub version: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateInstalled {
    pub version: String,
}

fn placeholder_active() -> bool {
    placeholder_in_place(TAURI_CONFIG)
}

fn configured_failure() -> String {
    FailureKind::NotConfigured.message().to_owned()
}

/// Whether an install may start for the version the user confirmed.
///
/// A release that moved between the check and the confirmation is refused
/// rather than installed: the user agreed to a specific version, and
/// "latest" is not what they read.
pub fn confirmable(offered_version: Option<&str>, confirmed_version: &str) -> Result<(), String> {
    match offered_version.map(str::trim) {
        Some(offered) if offered == confirmed_version.trim() => Ok(()),
        _ => Err(FailureKind::Changed.message().to_owned()),
    }
}

/// Current version, channel, and whether the updater can be trusted.
#[tauri::command]
pub fn app_update_status(app: AppHandle) -> AppUpdateStatus {
    let configured = !placeholder_active();
    AppUpdateStatus {
        configured,
        current_version: app.package_info().version.to_string(),
        channel: CHANNEL,
        detail: (!configured).then(configured_failure),
    }
}

/// Check the endpoint. Runs only because the user clicked the button.
#[tauri::command]
pub async fn app_update_check(
    app: AppHandle,
    request: AppUpdateCheckRequest,
) -> Result<AppUpdateCheck, String> {
    let current_version = app.package_info().version.to_string();
    if placeholder_active() {
        return Ok(AppUpdateCheck {
            status: CheckStatus::NotConfigured,
            current_version,
            version: None,
            notes: None,
            published_at: None,
            detail: Some(configured_failure()),
        });
    }

    let updater = app
        .updater()
        .map_err(|error| FailureKind::from_updater_error(&error).message().to_owned())?;
    let announced = updater
        .check()
        .await
        .map_err(|error| FailureKind::from_updater_error(&error).message().to_owned())?;

    let notes = announced.as_ref().and_then(|update| update.body.clone());
    let published_at = announced
        .as_ref()
        .and_then(|update| update.date)
        .map(|date| date.to_string());
    let announced_version = announced.as_ref().map(|update| update.version.as_str());

    let (status, version, detail) = match resolve(
        &current_version,
        announced_version,
        request.declined_version.as_deref(),
    ) {
        Resolution::Available { version } => (CheckStatus::Available, Some(version), None),
        Resolution::Declined { version } => (CheckStatus::Declined, Some(version), None),
        Resolution::UpToDate => (CheckStatus::UpToDate, None, None),
        Resolution::Unavailable { reason } => {
            let detail = match reason {
                Refusal::Prerelease => format!(
                    "Version {} is a preview build. Moddin only installs stable releases from the {CHANNEL} channel.",
                    announced_version.unwrap_or_default()
                ),
                Refusal::UnreadableVersion => format!(
                    "Version {} could not be read, so Moddin will not offer it.",
                    announced_version.unwrap_or_default()
                ),
            };
            (CheckStatus::Unavailable, None, Some(detail))
        }
    };

    Ok(AppUpdateCheck {
        status,
        current_version,
        version,
        notes,
        published_at,
        detail,
    })
}

/// Download, verify and install the version the user confirmed.
///
/// The check is repeated here on purpose: the artifact that gets installed
/// is the one the endpoint serves at this moment, and it has to be the
/// version that was on screen when they said yes.
#[tauri::command]
pub async fn app_update_install(
    app: AppHandle,
    request: AppUpdateInstallRequest,
) -> Result<AppUpdateInstalled, String> {
    if placeholder_active() {
        return Err(configured_failure());
    }

    let updater = app
        .updater()
        .map_err(|error| FailureKind::from_updater_error(&error).message().to_owned())?;
    let update = updater
        .check()
        .await
        .map_err(|error| FailureKind::from_updater_error(&error).message().to_owned())?
        .ok_or_else(|| FailureKind::Manifest.message().to_owned())?;

    confirmable(Some(update.version.as_str()), &request.version)?;

    let version = update.version.clone();
    let progress_handle = app.clone();
    let on_chunk = move |chunk: usize, total: Option<u64>| {
        let _ = progress_handle.emit(
            PROGRESS_EVENT,
            AppUpdateProgress {
                phase: "downloading",
                downloaded: chunk as u64,
                total,
            },
        );
    };
    let installed_handle = app.clone();
    let on_download_finish = move || {
        let _ = installed_handle.emit(
            PROGRESS_EVENT,
            AppUpdateProgress {
                phase: "installing",
                downloaded: 0,
                total: None,
            },
        );
    };

    // On Windows this does not return on success: the plugin launches the
    // NSIS installer, which restarts Moddin, and the process exits here.
    // Every failure below it returns normally with the installed version
    // untouched.
    update
        .download_and_install(on_chunk, on_download_finish)
        .await
        .map_err(|error| FailureKind::from_updater_error(&error).message().to_owned())?;

    Ok(AppUpdateInstalled { version })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The version this build reports, so the tests read as decisions
    /// about one app rather than about version strings in the air.
    const CURRENT: &str = "0.1.0";

    #[test]
    fn offers_a_newer_stable_release() {
        assert_eq!(
            decide(CURRENT, "0.2.0"),
            Offer::Available {
                version: "0.2.0".to_owned()
            }
        );
        assert_eq!(
            decide(CURRENT, "0.1.1"),
            Offer::Available {
                version: "0.1.1".to_owned()
            }
        );
    }

    #[test]
    fn does_not_offer_the_same_or_an_older_release() {
        assert_eq!(decide(CURRENT, "0.1.0"), Offer::UpToDate);
        assert_eq!(decide(CURRENT, "0.0.9"), Offer::UpToDate);
        // A `v` prefix is how Git tags are written; the version the
        // manifest carries is not.
        assert_eq!(decide(CURRENT, "v0.1.0"), Offer::UpToDate);
    }

    #[test]
    fn refuses_a_prerelease_however_new_it_is() {
        assert_eq!(
            decide(CURRENT, "9.0.0-rc.1"),
            Offer::Refused(Refusal::Prerelease)
        );
        assert_eq!(
            decide(CURRENT, "0.1.1-beta.2"),
            Offer::Refused(Refusal::Prerelease)
        );
    }

    #[test]
    fn refuses_a_version_it_cannot_compare() {
        assert_eq!(
            decide(CURRENT, "nightly"),
            Offer::Refused(Refusal::UnreadableVersion)
        );
        assert_eq!(
            decide("nightly", "0.2.0"),
            Offer::Refused(Refusal::UnreadableVersion)
        );
    }

    #[test]
    fn a_declined_version_is_not_offered_again() {
        // The whole "do not nag" rule. There is no timer to suppress:
        // this is what the panel asks for on the next manual check.
        assert_eq!(
            resolve(CURRENT, Some("0.2.0"), Some("0.2.0")),
            Resolution::Declined {
                version: "0.2.0".to_owned()
            }
        );
        // Declining 0.1.9 does not decline 0.2.0.
        assert_eq!(
            resolve(CURRENT, Some("0.2.0"), Some("0.1.9")),
            Resolution::Available {
                version: "0.2.0".to_owned()
            }
        );
        // And a declined version that is no longer announced is just up to
        // date, not a pending offer.
        assert_eq!(
            resolve(CURRENT, Some("0.1.0"), Some("0.2.0")),
            Resolution::UpToDate
        );
    }

    #[test]
    fn an_empty_endpoint_answer_is_up_to_date() {
        assert_eq!(resolve(CURRENT, None, None), Resolution::UpToDate);
        assert_eq!(resolve(CURRENT, Some("  "), None), Resolution::UpToDate);
    }

    #[test]
    fn a_signature_failure_is_a_refusal_and_not_a_fallback() {
        // `Minisign` is the variant a real bad signature arrives as. Its
        // siblings are the same refusal, and they are constructible here,
        // so the classification is asserted on real values.
        assert_eq!(
            FailureKind::from_updater_error(&UpdaterError::SignatureUtf8("not base64".to_owned())),
            FailureKind::Signature
        );
        let message = FailureKind::Signature.message();
        assert!(
            message.contains("refused"),
            "a bad signature has to be named as a refusal: {message}"
        );
        assert!(message.contains("Nothing was installed"), "{message}");
        // The other half of "not a fallback": with no artifact for the
        // confirmed version, the install refuses. There is no path that
        // proceeds to an unverified file.
        assert_eq!(
            confirmable(None, "0.2.0"),
            Err(FailureKind::Changed.message().to_owned())
        );
    }

    #[test]
    fn install_failures_are_classified_by_what_they_are() {
        assert_eq!(
            FailureKind::from_updater_error(&UpdaterError::Network("timeout".to_owned())),
            FailureKind::Download
        );
        assert_eq!(
            FailureKind::from_updater_error(&UpdaterError::ReleaseNotFound),
            FailureKind::Manifest
        );
        assert_eq!(
            FailureKind::from_updater_error(&UpdaterError::TargetNotFound(
                "windows-x86_64".to_owned()
            )),
            FailureKind::Manifest
        );
        assert_eq!(
            FailureKind::from_updater_error(&UpdaterError::PackageInstallFailed),
            FailureKind::Install
        );
        assert_eq!(
            FailureKind::from_updater_error(&UpdaterError::InsecureTransportProtocol),
            FailureKind::Manifest
        );
    }

    #[test]
    fn every_failure_message_says_nothing_was_installed() {
        // The promise the UI makes in each case, asserted once so a new
        // kind cannot be added without saying what the user keeps.
        for kind in [
            FailureKind::NotConfigured,
            FailureKind::Signature,
            FailureKind::Download,
            FailureKind::Manifest,
            FailureKind::Install,
            FailureKind::Changed,
            FailureKind::Unknown,
        ] {
            let message = kind.message();
            assert!(!message.is_empty(), "{kind:?} has no message");
            // Case-insensitive because the sentence can start lower-case
            // mid-paragraph; the promise is what matters, not the casing.
            let said = message.to_ascii_lowercase();
            assert!(
                said.contains("nothing was") || said.contains("left untouched"),
                "{kind:?} does not say the installed version survived: {message}"
            );
        }
    }

    #[test]
    fn a_release_that_moved_is_not_installed() {
        assert_eq!(
            confirmable(Some("0.3.0"), "0.2.0"),
            Err(FailureKind::Changed.message().to_owned())
        );
        assert_eq!(confirmable(Some("0.2.0"), "0.2.0"), Ok(()));
    }

    #[test]
    fn the_placeholder_marker_is_found_in_a_placeholder_config() {
        assert!(placeholder_in_place(
            r#""pubkey": "REPLACE_WITH_TAURI_UPDATER_PUBKEY: paste the real key""#
        ));
        assert!(!placeholder_in_place(
            r#""pubkey": "minisign+PUBLIC KEY RWTY1kI1FjX0RTZ2FsT2p4VnA""#
        ));
        assert!(
            !placeholder_in_place(""),
            "a config too short to hold the marker is not a placeholder"
        );
    }
}
