//! Archive source abstraction.
//!
//! Several modules need to fetch a downloadable archive (ReShade, UEVR,
//! OFXR, future UE4SS, BepInEx, REFramework) and verify its SHA-256
//! against a recipe-declared digest. Before this trait each module
//! reimplemented the same dance: validate URL or local path, read
//! bytes, hash, compare to expected, surface a clear error.
//!
//! [`ArchiveSource`] factors the shared shape:
//!
//! - [`LocalArchive`] — bytes come from a path on disk.
//! - [`RemoteArchive`] — bytes come from an HTTPS GET.
//!
//! New sources (S3, B2, signed URLs) slot in by implementing
//! [`ArchiveSource`].
//!
//! ## Where SHA-256 is actually checked
//!
//! Not here. `fetch` returns `(bytes, computed_sha256)` and the *caller*
//! compares, because the two call sites need different behaviour:
//! `builtin_checks::evaluate_archive_sha256` does the preflight comparison
//! against the recipe's declared digest, and the `verify-hash` step does
//! it at install time. This file used to carry an `expected_sha256` field
//! and a `fetch_verified` wrapper to do the comparison inside the trait,
//! but nothing ever called `with_expected_sha256`, so the expected digest
//! was always `None` and `fetch_verified` was `fetch` with a branch that
//! could never be taken.

use reqwest::Client;
use sha2::{Digest, Sha256};
// Only `LocalArchive` opens a file, and it is test-only.
#[cfg(test)]
use std::fs::File;
// Only `LocalArchive` reads a file to the end, and it is test-only.
#[cfg(test)]
use std::io::Read;

/// Maximum archive size, enforced by both implementations. Moddin
/// archives are well under this (ReShade ~6 MB, UEVR ~10 MB,
/// OFXR ~50 MB) but a sane ceiling guards against a misconfigured
/// `update_url` pointing at a multi-gigabyte file.
pub const MAX_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;

/// Common contract for any source of archive bytes.
///
/// `async_fn_in_trait` is allowed deliberately: the trait is `dyn`-less by
/// design and only ever used as a generic bound
/// (`archive: &impl ArchiveSource`), so it does not need `-> impl Future`
/// in return position. The `Send`-bound caveat of bare AFIT does not apply
/// because the only callers await the future inline on the same task.
#[allow(async_fn_in_trait, reason = "generic-only bound, never used as dyn")]
pub trait ArchiveSource: Send + Sync {
    /// Fetch the archive contents and return `(bytes, computed_sha256)`.
    async fn fetch(&self) -> Result<(Vec<u8>, String), String>;

    /// Short label for log lines and error messages (e.g. `"local:..."`,
    /// `"https://..."`).
    fn label(&self) -> String;
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

/// Reads an archive from a local filesystem path.
///
/// Test-only for now: every production call site fetches over HTTPS
/// (`RemoteArchive`), so nothing outside the test module constructs this.
#[cfg(test)]
pub struct LocalArchive {
    pub path: std::path::PathBuf,
}

#[cfg(test)]
impl LocalArchive {
    pub fn new(path: impl Into<std::path::PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

#[cfg(test)]
impl ArchiveSource for LocalArchive {
    async fn fetch(&self) -> Result<(Vec<u8>, String), String> {
        if !self.path.is_file() {
            return Err(format!("{}: local archive does not exist.", self.label()));
        }
        let mut file = File::open(&self.path)
            .map_err(|error| format!("{}: could not open archive: {error}", self.label()))?;
        let metadata = file
            .metadata()
            .map_err(|error| format!("{}: could not stat archive: {error}", self.label()))?;
        if metadata.len() > MAX_ARCHIVE_BYTES {
            return Err(format!(
                "{}: archive exceeds the {MAX_ARCHIVE_BYTES}-byte safety limit.",
                self.label()
            ));
        }
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        file.read_to_end(&mut bytes)
            .map_err(|error| format!("{}: could not read archive: {error}", self.label()))?;
        let digest = sha256_hex(&bytes);
        Ok((bytes, digest))
    }

    fn label(&self) -> String {
        format!("local:{}", self.path.display())
    }
}

/// Fetches an archive over HTTPS and verifies the response.
pub struct RemoteArchive {
    pub url: String,
    pub host_allowlist: Vec<String>,
}

impl RemoteArchive {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            host_allowlist: Vec::new(),
        }
    }

    /// Restrict the allowed hosts for the download (e.g.
    /// `["github.com", "objects.githubusercontent.com"]`). When the list
    /// is empty, the only allowed host is `github.com`.
    pub fn with_host_allowlist(mut self, hosts: Vec<String>) -> Self {
        self.host_allowlist = hosts;
        self
    }

    /// Size of the remote payload in bytes, from a HEAD request, or
    /// `None` when the server does not report one.
    ///
    /// Exists so a preflight check can decide *whether to download*
    /// without starting the download. `archive-sha256` is a blocker
    /// that runs while the UI is rendering a module card, and pulling
    /// 300 MB to decide whether a button should be enabled is not a
    /// check.
    ///
    /// `Err` means the size could not be determined (offline, host not
    /// allowed, server rejects HEAD). Callers should treat that as
    /// "unknown size", not as "download failed".
    pub async fn content_length(&self) -> Result<Option<u64>, String> {
        let parsed = reqwest::Url::parse(&self.url)
            .map_err(|error| format!("{}: invalid URL: {error}", self.label()))?;
        if parsed.scheme() != "https" {
            return Err(format!("{}: must use HTTPS.", self.label()));
        }
        let host = parsed
            .host_str()
            .ok_or_else(|| format!("{}: missing host.", self.label()))?
            .to_ascii_lowercase();
        let allowed = if self.host_allowlist.is_empty() {
            vec!["github.com".to_owned()]
        } else {
            self.host_allowlist
                .iter()
                .map(|host| host.to_ascii_lowercase())
                .collect()
        };
        if !allowed.contains(&host) {
            return Err(format!(
                "{}: host '{host}' is not in the allow-list.",
                self.label()
            ));
        }

        let client = Client::builder()
            .user_agent("Moddin-Desktop/0.1 (+https://github.com/petonexus/moddin-desktop)")
            .build()
            .map_err(|error| format!("{}: could not create downloader: {error}", self.label()))?;

        let response =
            client.head(parsed).send().await.map_err(|error| {
                format!("{}: could not read archive size: {error}", self.label())
            })?;

        Ok(response.content_length())
    }
}

impl ArchiveSource for RemoteArchive {
    async fn fetch(&self) -> Result<(Vec<u8>, String), String> {
        let parsed = reqwest::Url::parse(&self.url)
            .map_err(|error| format!("{}: invalid URL: {error}", self.label()))?;
        if parsed.scheme() != "https" {
            return Err(format!("{}: must use HTTPS.", self.label()));
        }
        let host = parsed
            .host_str()
            .ok_or_else(|| format!("{}: missing host.", self.label()))?
            .to_ascii_lowercase();
        let allowed = if self.host_allowlist.is_empty() {
            vec!["github.com".to_owned()]
        } else {
            self.host_allowlist
                .iter()
                .map(|host| host.to_ascii_lowercase())
                .collect()
        };
        if !allowed.contains(&host) {
            return Err(format!(
                "{}: host '{host}' is not in the allow-list.",
                self.label()
            ));
        }
        if !parsed.username().is_empty() || parsed.password().is_some() {
            return Err(format!(
                "{}: credentials in URL are not allowed.",
                self.label()
            ));
        }

        let client = Client::builder()
            .user_agent("Moddin-Desktop/0.1 (+https://github.com/petonexus/moddin-desktop)")
            .build()
            .map_err(|error| format!("{}: could not create downloader: {error}", self.label()))?;

        let response = client
            .get(parsed.clone())
            .send()
            .await
            .map_err(|error| format!("{}: download failed: {error}", self.label()))?
            .error_for_status()
            .map_err(|error| format!("{}: server returned an error: {error}", self.label()))?;

        let bytes = response
            .bytes()
            .await
            .map_err(|error| format!("{}: could not read response bytes: {error}", self.label()))?;
        if bytes.len() as u64 > MAX_ARCHIVE_BYTES {
            return Err(format!(
                "{}: archive exceeds the {MAX_ARCHIVE_BYTES}-byte safety limit.",
                self.label()
            ));
        }
        Ok((bytes.to_vec(), sha256_hex(&bytes)))
    }

    fn label(&self) -> String {
        self.url.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn local_archive_rejects_missing_path() {
        let source = LocalArchive::new(std::path::PathBuf::from("C:/no/such/file.zip"));
        let result = source.fetch().await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("does not exist"));
    }

    #[tokio::test]
    async fn remote_archive_rejects_http() {
        let source = RemoteArchive::new("http://github.com/foo/bar.zip");
        let result = source.fetch().await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("HTTPS"));
    }

    #[tokio::test]
    async fn remote_archive_rejects_non_allowlisted_host() {
        let source = RemoteArchive::new("https://example.com/foo.zip")
            .with_host_allowlist(vec!["github.com".to_owned()]);
        let result = source.fetch().await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not in the allow-list"));
    }

    #[test]
    fn sha256_hex_matches_known_value() {
        // SHA-256 of empty bytes.
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
