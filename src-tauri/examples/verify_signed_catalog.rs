//! Verify a hand-signed or CI-signed catalog.json against the
//! pinned public key. Standalone smoke test (no Tauri runtime).
//!
//! Usage:
//!     cargo run --example verify_signed_catalog -- \
//!         <path-to-catalog.json> <path-to-catalog.json.sig>

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use std::{fs, path::PathBuf};

fn main() {
    let mut args = std::env::args().skip(1);
    let catalog_path: PathBuf = args
        .next()
        .map(PathBuf::from)
        .expect("usage: cargo run --example verify_signed_catalog -- <catalog> <sig>");
    let sig_path: PathBuf = args
        .next()
        .map(PathBuf::from)
        .expect("usage: cargo run --example verify_signed_catalog -- <catalog> <sig>");

    let public_b64 = std::env::var("MODDIN_BOOTSTRAP_KEY")
        .expect("set MODDIN_BOOTSTRAP_KEY to the base64 raw 32-byte Ed25519 public key");
    let public_bytes = BASE64.decode(public_b64.trim()).expect("invalid base64");
    assert_eq!(public_bytes.len(), 32, "expected 32-byte Ed25519 public key");
    let key_array: [u8; 32] = public_bytes
        .as_slice()
        .try_into()
        .expect("convert to fixed array");
    let key = VerifyingKey::from_bytes(&key_array).expect("invalid Ed25519 key");

    let catalog_bytes = fs::read(&catalog_path).expect("could not read catalog");
    let sig_b64 = fs::read_to_string(&sig_path).expect("could not read signature");
    let sig_bytes = BASE64.decode(sig_b64.trim()).expect("signature is not valid base64");
    assert_eq!(sig_bytes.len(), 64, "expected 64-byte Ed25519 signature");
    let sig_array: [u8; 64] = sig_bytes
        .as_slice()
        .try_into()
        .expect("convert to fixed array");
    let signature = Signature::from_bytes(&sig_array);

    match key.verify(&catalog_bytes, &signature) {
        Ok(()) => println!(
            "[OK] {} ({} bytes) signed by {} bytes signature",
            catalog_path.display(),
            catalog_bytes.len(),
            sig_bytes.len(),
        ),
        Err(error) => {
            eprintln!(
                "[FAIL] {} verification failed: {error}",
                catalog_path.display()
            );
            std::process::exit(1);
        }
    }
}
