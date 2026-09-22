//! Generate a fresh Ed25519 keypair for the Moddin Desktop community
//! catalog maintainer. Run once with:
//!
//!     cargo run --example gen_maintainer_key
//!
//! Prints three blocks to stdout:
//!
//! 1. **PUBLIC_KEY_B64** — base64 of the raw 32-byte public key.
//!    Paste this into `BOOTSTRAP_PUBLIC_KEY_B64` in
//!    `src-tauri/src/community_catalog.rs` AND into
//!    `public-keys.json` in the community repo.
//!
//! 2. **MAINTAINER_PRIVATE_KEY_PEM** — PEM-encoded private key.
//!    Set this as the `MODDIN_CATALOG_SIGNING_KEY` repository
//!    secret on the community repo so `build-catalog.yml` can sign
//!    `catalog.json` after each merge.
//!
//! 3. **FINGERPRINT** — first 16 hex chars of SHA-256(pubkey).
//!    Use this in MAINTAINERS.md / SECURITY.md to identify the
//!    rotating key without exposing the private key.
//!
//! The example is `pub`-eligible so anyone can run it; nothing in
//! here depends on the app state. Store the printed private key
//! somewhere safe (password manager, encrypted disk) — losing it
//! means losing the ability to ship signed catalog updates until
//! the maintainer rotates to a new key.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ed25519_dalek::{pkcs8::EncodePrivateKey, SigningKey};
use pkcs8::LineEnding;
use rand::RngCore;
use sha2::{Digest, Sha256};

fn main() {
    let mut rng = rand::thread_rng();
    let mut secret = [0u8; 32];
    rng.fill_bytes(&mut secret);
    let signing_key = SigningKey::from_bytes(&secret);
    let verifying_key = signing_key.verifying_key();

    let public_b64 = BASE64.encode(verifying_key.to_bytes());
    let private_pem = signing_key
        .to_pkcs8_pem(LineEnding::LF)
        .expect("pkcs8 export")
        .to_string();

    let mut hasher = Sha256::new();
    hasher.update(verifying_key.to_bytes());
    let digest = hasher.finalize();
    let fingerprint = digest
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();

    println!("# === Generated Moddin maintainer keypair ===");
    println!("# Generated at: {}", chrono::Utc::now().to_rfc3339());
    println!();
    println!("PUBLIC_KEY_B64={public_b64}");
    println!();
    println!("# PEM-encoded PKCS#8 private key (set as MODDIN_CATALOG_SIGNING_KEY secret)");
    println!("MAINTAINER_PRIVATE_KEY_PEM=\"\"\"");
    println!("{private_pem}");
    println!("\"\"\"");
    println!();
    println!("FINGERPRINT={fingerprint}");
}
