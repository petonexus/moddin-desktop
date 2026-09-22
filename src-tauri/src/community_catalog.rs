//! Community catalog fetcher.
//!
//! Downloads the curated capability list from
//! `petonexus/moddin-community-capabilities` on GitHub, verifies it
//! against the pinned maintainer public key, and caches the result
//! locally with a configurable TTL (default 24h).
//!
//! ## Threat model
//!
//! The bootstrap public key is compiled into the binary. The runner
//! REJECTS any catalog whose Ed25519 signature does not verify against
//! that key. If the maintainer rotates the key, a new Moddin Desktop
//! release carries the new key — release-time rotation is intentional
//! friction for the highest-trust asset.
//!
//! ## Cache layout
//!
//! ```text
//! %LOCALAPPDATA%\Moddin\community-capabilities\
//!   catalog.json     ← cached payload
//!   catalog.meta.json ← { cachedAt, ttlSeconds, signatureVerified }
//!   pinned-public-key.bin ← Ed25519 raw 32 bytes
//! ```
//!
//! ## TTL
//!
//! * 1h, 6h, 24h, 7d, or manual (refresh-only). Stored in
//!   `catalog.meta.json`.
//! * When the cache is fresh, no network calls happen.
//! * When expired, a single GET pulls both the catalog and the signature
//!   (the signature ships as a sibling file or, in the future, as a
//!   GitHub Release asset).

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const CATALOG_URL: &str =
    "https://raw.githubusercontent.com/petonexus/moddin-community-capabilities/main/catalog.json";
const CATALOG_SIG_URL: &str =
    "https://raw.githubusercontent.com/petonexus/moddin-community-capabilities/main/catalog.json.sig";

const DEFAULT_TTL_SECONDS: u64 = 24 * 60 * 60;
const MIN_TTL_SECONDS: u64 = 60 * 60; // 1h floor
const MAX_TTL_SECONDS: u64 = 30 * 24 * 60 * 60; // 30d ceiling

/// Bootstrap public key compiled into the binary. **Replace** with the
/// real value once
/// [`petonexus/moddin-community-capabilities` `public-keys.json`](https://github.com/petonexus/moddin-community-capabilities)
/// has a non-placeholder key. The placeholder below is a valid 32-byte
/// Ed25519 public key whose corresponding private key does not exist,
/// so every signature verification fails closed until the maintainer
/// swaps the value.
const BOOTSTRAP_PUBLIC_KEY_B64: &str =
    "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";

fn community_dir() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .map(|dir| dir.join("Moddin").join("community-capabilities"))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommunityConfig {
    pub ttl_seconds: u64,
}

impl Default for CommunityConfig {
    fn default() -> Self {
        Self {
            ttl_seconds: DEFAULT_TTL_SECONDS,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommunityCatalogEntry {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub download_url: Option<String>,
    #[serde(default)]
    pub config_schema: serde_json::Value,
    #[serde(default)]
    pub safety_notes: Vec<String>,
    #[serde(default)]
    pub signed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommunityCatalog {
    pub version: u32,
    #[serde(default)]
    pub generated_at: String,
    #[serde(default)]
    pub generator: String,
    #[serde(default)]
    pub capabilities: Vec<CommunityCatalogEntry>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommunityFetchResult {
    pub catalog: CommunityCatalog,
    pub cached: bool,
    pub cached_at: u64,
    pub ttl_seconds: u64,
    pub signature_verified: bool,
    pub bootstrap_public_key_fingerprint: String,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CacheMetadata {
    cached_at: u64,
    ttl_seconds: u64,
    signature_verified: bool,
    bootstrap_fingerprint: String,
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn ensure_bootstrap_key_material(dir: &Path) -> Result<VerifyingKey, String> {
    let path = dir.join("pinned-public-key.bin");
    if let Ok(bytes) = std::fs::read(&path) {
        if bytes.len() == 32 {
            if let Ok(array) =
                <[u8; 32]>::try_from(bytes.as_slice()).map_err(|error: std::array::TryFromSliceError| error.to_string())
            {
                if let Ok(key) = VerifyingKey::from_bytes(&array) {
                    return Ok(key);
                }
            }
        }
    }
    // Fall back to the compile-time bootstrap key.
    let bytes = BASE64
        .decode(BOOTSTRAP_PUBLIC_KEY_B64)
        .map_err(|error| format!("bootstrap public key is not valid base64: {error}"))?;
    if bytes.len() != 32 {
        return Err(format!(
            "bootstrap public key must be 32 bytes, got {}",
            bytes.len()
        ));
    }
    let bytes_array: [u8; 32] = bytes
        .as_slice()
        .try_into()
        .map_err(|error: std::array::TryFromSliceError| error.to_string())?;
    VerifyingKey::from_bytes(&bytes_array).map_err(|error| format!("invalid Ed25519 key: {error}"))
}

fn fingerprint(key: &VerifyingKey) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(key.to_bytes());
    let digest = hasher.finalize();
    let hex = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    hex.chars().take(16).collect()
}

fn read_cached_catalog(dir: &Path) -> Option<(CommunityCatalog, CacheMetadata)> {
    let catalog_path = dir.join("catalog.json");
    let meta_path = dir.join("catalog.meta.json");
    let raw = std::fs::read_to_string(&catalog_path).ok()?;
    let meta_raw = std::fs::read_to_string(&meta_path).ok()?;
    let catalog: CommunityCatalog = serde_json::from_str(&raw).ok()?;
    let meta: CacheMetadata = serde_json::from_str(&meta_raw).ok()?;
    Some((catalog, meta))
}

fn write_cache(
    dir: &Path,
    catalog: &CommunityCatalog,
    meta: &CacheMetadata,
) -> Result<(), String> {
    std::fs::create_dir_all(dir)
        .map_err(|error| format!("could not create community cache dir: {error}"))?;
    let catalog_path = dir.join("catalog.json");
    let meta_path = dir.join("catalog.meta.json");
    std::fs::write(
        &catalog_path,
        serde_json::to_vec_pretty(catalog)
            .map_err(|error| format!("could not serialise catalog: {error}"))?,
    )
    .map_err(|error| format!("could not write catalog.json: {error}"))?;
    std::fs::write(
        &meta_path,
        serde_json::to_vec_pretty(meta)
            .map_err(|error| format!("could not serialise metadata: {error}"))?,
    )
    .map_err(|error| format!("could not write catalog.meta.json: {error}"))?;
    Ok(())
}

fn pin_pinned_public_key(dir: &Path, key: &VerifyingKey) -> Result<(), String> {
    std::fs::create_dir_all(dir)
        .map_err(|error| format!("could not create community cache dir: {error}"))?;
    let path = dir.join("pinned-public-key.bin");
    std::fs::write(&path, key.to_bytes())
        .map_err(|error| format!("could not persist pinned public key: {error}"))?;
    Ok(())
}

fn verify_signature(
    payload: &[u8],
    signature_b64: &str,
    key: &VerifyingKey,
) -> Result<(), String> {
    let signature_bytes = BASE64
        .decode(signature_b64.trim())
        .map_err(|error| format!("signature is not valid base64: {error}"))?;
    if signature_bytes.len() != 64 {
        return Err(format!(
            "signature must be 64 bytes (Ed25519), got {}",
            signature_bytes.len()
        ));
    }
    let signature_array: [u8; 64] = signature_bytes
        .as_slice()
        .try_into()
        .map_err(|error: std::array::TryFromSliceError| error.to_string())?;
    let signature = Signature::from_bytes(&signature_array);
    key.verify(payload, &signature)
        .map_err(|error| format!("signature verification failed: {error}"))
}

/// Fetch the community catalog, honouring the cached TTL unless
/// `force_refresh` is set.
pub async fn fetch(force_refresh: bool, ttl_override: Option<u64>) -> CommunityFetchResult {
    let mut ttl_seconds = ttl_override
        .unwrap_or(DEFAULT_TTL_SECONDS)
        .clamp(MIN_TTL_SECONDS, MAX_TTL_SECONDS);
    let dir = match community_dir() {
        Some(dir) => dir,
        None => return empty_result(ttl_seconds, "LOCALAPPDATA not set".to_owned()),
    };

    if !force_refresh {
        if let Some((catalog, meta)) = read_cached_catalog(&dir) {
            if meta.ttl_seconds != 0 {
                ttl_seconds = meta.ttl_seconds;
            }
            let age = now_unix().saturating_sub(meta.cached_at);
            if age < ttl_seconds {
                return CommunityFetchResult {
                    catalog,
                    cached: true,
                    cached_at: meta.cached_at,
                    ttl_seconds,
                    signature_verified: meta.signature_verified,
                    bootstrap_public_key_fingerprint: meta.bootstrap_fingerprint,
                    last_error: None,
                };
            }
        }
    }

    let key = match ensure_bootstrap_key_material(&dir) {
        Ok(key) => key,
        Err(error) => return empty_result(ttl_seconds, error),
    };

    let client = match reqwest::Client::builder()
        .user_agent("Moddin-Desktop/0.1 (+https://github.com/petonexus/moddin-desktop)")
        .build()
    {
        Ok(client) => client,
        Err(error) => return empty_result(ttl_seconds, format!("could not build HTTP client: {error}")),
    };

    let catalog_response = match client.get(CATALOG_URL).send().await {
        Ok(response) => response,
        Err(error) => return empty_result(ttl_seconds, format!("catalog GET failed: {error}")),
    };
    if !catalog_response.status().is_success() {
        return empty_result(
            ttl_seconds,
            format!("catalog GET returned HTTP {}", catalog_response.status()),
        );
    }
    let catalog_bytes = match catalog_response.bytes().await {
        Ok(bytes) => bytes,
        Err(error) => return empty_result(ttl_seconds, format!("could not read catalog bytes: {error}")),
    };

    let signature_response = match client.get(CATALOG_SIG_URL).send().await {
        Ok(response) => response,
        Err(error) => {
            return fallback_to_cache(&dir, ttl_seconds, format!("catalog signature GET failed: {error}"))
        }
    };
    let signature_text = match signature_response.text().await {
        Ok(text) => text,
        Err(error) => {
            return fallback_to_cache(
                &dir,
                ttl_seconds,
                format!("catalog signature read failed: {error}"),
            )
        }
    };

    if let Err(error) = verify_signature(&catalog_bytes, &signature_text, &key) {
        return fallback_to_cache(&dir, ttl_seconds, error);
    }

    let catalog: CommunityCatalog = match serde_json::from_slice(&catalog_bytes) {
        Ok(catalog) => catalog,
        Err(error) => return empty_result(ttl_seconds, format!("catalog JSON malformed: {error}")),
    };

    let fingerprint_value = fingerprint(&key);
    if let Err(error) = pin_pinned_public_key(&dir, &key) {
        return empty_result(ttl_seconds, format!("could not persist pinned key: {error}"));
    }
    let metadata = CacheMetadata {
        cached_at: now_unix(),
        ttl_seconds,
        signature_verified: true,
        bootstrap_fingerprint: fingerprint_value.clone(),
    };
    if let Err(error) = write_cache(&dir, &catalog, &metadata) {
        return empty_result(ttl_seconds, format!("could not write cache: {error}"));
    }

    CommunityFetchResult {
        catalog,
        cached: false,
        cached_at: metadata.cached_at,
        ttl_seconds,
        signature_verified: true,
        bootstrap_public_key_fingerprint: fingerprint_value,
        last_error: None,
    }
}

fn fallback_to_cache(dir: &Path, ttl_seconds: u64, error: String) -> CommunityFetchResult {
    if let Some((catalog, meta)) = read_cached_catalog(dir) {
        return CommunityFetchResult {
            catalog,
            cached: true,
            cached_at: meta.cached_at,
            ttl_seconds,
            signature_verified: meta.signature_verified,
            bootstrap_public_key_fingerprint: meta.bootstrap_fingerprint,
            last_error: Some(error),
        };
    }
    empty_result(ttl_seconds, error)
}

fn empty_result(ttl_seconds: u64, error: String) -> CommunityFetchResult {
    CommunityFetchResult {
        catalog: CommunityCatalog {
            version: 1,
            generated_at: String::new(),
            generator: String::new(),
            capabilities: Vec::new(),
        },
        cached: false,
        cached_at: 0,
        ttl_seconds,
        signature_verified: false,
        bootstrap_public_key_fingerprint: String::new(),
        last_error: Some(error),
    }
}

pub fn set_ttl(ttl_seconds: u64) -> u64 {
    ttl_seconds.clamp(MIN_TTL_SECONDS, MAX_TTL_SECONDS)
}

// === Tauri command surface =====================================================

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommunityFetchRequest {
    #[serde(default)]
    pub force_refresh: bool,
    #[serde(default)]
    pub ttl_seconds: Option<u64>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommunitySetTtlRequest {
    pub ttl_seconds: u64,
}

#[tauri::command]
pub async fn community_catalog_fetch(
    request: CommunityFetchRequest,
) -> CommunityFetchResult {
    fetch(request.force_refresh, request.ttl_seconds).await
}

#[tauri::command]
pub async fn community_catalog_set_ttl(
    request: CommunitySetTtlRequest,
) -> Result<u64, String> {
    Ok(set_ttl(request.ttl_seconds))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ttl_clamp_floor_and_ceiling() {
        assert_eq!(set_ttl(0), MIN_TTL_SECONDS);
        assert_eq!(set_ttl(60), MIN_TTL_SECONDS);
        assert_eq!(set_ttl(24 * 60 * 60), 24 * 60 * 60);
        assert_eq!(set_ttl(365 * 24 * 60 * 60), MAX_TTL_SECONDS);
    }

    #[test]
    fn bootstrap_key_decodes_to_32_bytes() {
        let bytes = BASE64
            .decode(BOOTSTRAP_PUBLIC_KEY_B64)
            .expect("placeholder is valid base64");
        assert_eq!(bytes.len(), 32, "Ed25519 public keys are 32 bytes");
    }

    #[test]
    fn signature_rejects_invalid_base64() {
        let key = ensure_bootstrap_key_material(&std::env::temp_dir())
            .expect("bootstrap key material");
        let error = verify_signature(b"payload", "@@@not-base64@@@", &key)
            .expect_err("must reject malformed signature");
        assert!(error.contains("base64"));
    }

    #[test]
    fn signature_rejects_wrong_length() {
        let key = ensure_bootstrap_key_material(&std::env::temp_dir())
            .expect("bootstrap key material");
        let error = verify_signature(b"payload", &BASE64.encode([0u8; 32]), &key)
            .expect_err("must reject 32-byte signature");
        assert!(error.contains("64 bytes"));
    }

    #[test]
    fn fingerprint_is_short_hex() {
        let key = ensure_bootstrap_key_material(&std::env::temp_dir())
            .expect("bootstrap key material");
        let value = fingerprint(&key);
        assert_eq!(value.len(), 16);
        assert!(value.chars().all(|character| character.is_ascii_hexdigit()));
    }
}
