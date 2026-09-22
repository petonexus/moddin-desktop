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
//! - [`VerifiedArchive`] — wraps another source and enforces the
//!   SHA-256 match against the expected digest.
//!
//! Modules compose these wrappers instead of branching over
//! `archive_url` vs `local_archive`. New sources (S3, B2, signed URLs)
//! slot in by implementing [`ArchiveSource`].

use reqwest::Client;
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read};

/// Maximum archive size, enforced by both implementations. Moddin
/// archives are well under this (ReShade ~6 MB, UEVR ~10 MB,
/// OFXR ~50 MB) but a sane ceiling guards against a misconfigured
/// `update_url` pointing at a multi-gigabyte file.
pub const MAX_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;

/// Common contract for any source of archive bytes.
#[allow(async_fn_in_trait)]
pub trait ArchiveSource: Send + Sync {
    /// Fetch the archive contents and return `(bytes, computed_sha256)`.
    async fn fetch(&self) -> Result<(Vec<u8>, String), String>;

    /// Short label for log lines and error messages (e.g. `"local:..."`,
    /// `"https://..."`).
    fn label(&self) -> String;

    /// Optional expected SHA-256; if set, the implementation MUST verify
    /// the computed hash against it before returning the bytes.
    fn expected_sha256(&self) -> Option<&str> {
        None
    }

    /// Fetch with the verification step applied (if `expected_sha256` is
    /// set). The default implementation calls `fetch` and verifies when
    /// needed; overriding is rarely necessary.
    async fn fetch_verified(&self) -> Result<(Vec<u8>, String), String> {
        let (bytes, computed) = self.fetch().await?;
        if let Some(expected) = self.expected_sha256() {
            if !computed.eq_ignore_ascii_case(expected) {
                return Err(format!(
                    "{}: SHA-256 mismatch (expected {}, got {}).",
                    self.label(),
                    expected,
                    computed
                ));
            }
        }
        Ok((bytes, computed))
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

/// Reads an archive from a local filesystem path.
pub struct LocalArchive {
    pub path: std::path::PathBuf,
    pub expected_sha256: Option<String>,
}

impl LocalArchive {
    pub fn new(path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            path: path.into(),
            expected_sha256: None,
        }
    }

    pub fn with_expected_sha256(mut self, expected: impl Into<String>) -> Self {
        self.expected_sha256 = Some(expected.into());
        self
    }
}

impl ArchiveSource for LocalArchive {
    async fn fetch(&self) -> Result<(Vec<u8>, String), String> {
        if !self.path.is_file() {
            return Err(format!(
                "{}: local archive does not exist.",
                self.label()
            ));
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

    fn expected_sha256(&self) -> Option<&str> {
        self.expected_sha256.as_deref()
    }
}

/// Fetches an archive over HTTPS and verifies the response.
pub struct RemoteArchive {
    pub url: String,
    pub host_allowlist: Vec<String>,
    pub expected_sha256: Option<String>,
}

impl RemoteArchive {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            host_allowlist: Vec::new(),
            expected_sha256: None,
        }
    }

    /// Restrict the allowed hosts for the download (e.g.
    /// `["github.com", "objects.githubusercontent.com"]`). When the list
    /// is empty, the only allowed host is `github.com`.
    pub fn with_host_allowlist(mut self, hosts: Vec<String>) -> Self {
        self.host_allowlist = hosts;
        self
    }

    pub fn with_expected_sha256(mut self, expected: impl Into<String>) -> Self {
        self.expected_sha256 = Some(expected.into());
        self
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
            self.host_allowlist.iter().map(|host| host.to_ascii_lowercase()).collect()
        };
        if !allowed.iter().any(|candidate| host == *candidate) {
            return Err(format!(
                "{}: host '{host}' is not in the allow-list.",
                self.label()
            ));
        }
        if !parsed.username().is_empty() || parsed.password().is_some() {
            return Err(format!("{}: credentials in URL are not allowed.", self.label()));
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

    fn expected_sha256(&self) -> Option<&str> {
        self.expected_sha256.as_deref()
    }
}

/// Compose an [`ArchiveSource`] with a SHA-256 expectation, when the
/// inner source doesn't already carry one. Useful for tests and for
/// callers that want to enforce verification without rebuilding the
/// inner source.
pub struct VerifiedArchive<S: ArchiveSource> {
    pub inner: S,
    pub expected_sha256: String,
}

impl<S: ArchiveSource> VerifiedArchive<S> {
    pub fn new(inner: S, expected: impl Into<String>) -> Self {
        Self {
            inner,
            expected_sha256: expected.into(),
        }
    }
}

impl<S: ArchiveSource + Send + Sync> ArchiveSource for VerifiedArchive<S> {
    async fn fetch(&self) -> Result<(Vec<u8>, String), String> {
        self.inner.fetch().await
    }

    fn label(&self) -> String {
        self.inner.label()
    }

    fn expected_sha256(&self) -> Option<&str> {
        Some(&self.expected_sha256)
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
        assert_eq!(sha256_hex(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    }

    #[test]
    fn verified_archive_propagates_expected_digest() {
        let inner = LocalArchive::new(std::path::PathBuf::from("C:/no/such/file.zip"));
        let verified = VerifiedArchive::new(inner, "0".repeat(64));
        assert_eq!(
            verified.expected_sha256(),
            Some("0000000000000000000000000000000000000000000000000000000000000000")
        );
        assert_eq!(verified.label(), "local:C:/no/such/file.zip");
    }
}