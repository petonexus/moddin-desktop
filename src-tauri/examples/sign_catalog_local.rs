//! Sign a community catalog.json with the local maintainer private
//! key. Stand-in for the `build-catalog.yml` workflow when iterating
//! locally before pushing to the community repo.
//!
//! Usage (from the repo root, after `git pull` on the community repo):
//!
//!     cd src-tauri
//!     cargo run --example sign_catalog_local -- \
//!         ../../moddin-community-capabilities/catalog.json
//!
//! Writes `<input>.sig` next to the input file using the Ed25519
//! private key printed by `gen_maintainer_key`. The signature matches
//! what the GitHub Actions workflow produces via `openssl pkeyutl -sign`.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ed25519_dalek::{
    pkcs8::DecodePrivateKey, Signature, Signer, SigningKey, Verifier,
};
use std::{fs, path::PathBuf};

fn main() {
    let mut args = std::env::args().skip(1);
    let catalog_path: PathBuf = args
        .next()
        .map(PathBuf::from)
        .expect("usage: cargo run --example sign_catalog_local -- <catalog.json>");
    let private_key_pem = std::env::var("MODDIN_SIGNING_KEY").unwrap_or_else(|_| {
        eprintln!(
            "MODDIN_SIGNING_KEY not set; paste the PEM block from \
             `cargo run --example gen_maintainer_key` (without the\n\
             BEGIN/END lines) into the env var and re-run."
        );
        std::process::exit(2);
    });

    let signing_key = SigningKey::from_pkcs8_pem(&private_key_pem)
        .expect("could not parse MODDIN_SIGNING_KEY as PKCS#8 PEM");
    let verifying_key = signing_key.verifying_key();

    let catalog_bytes = fs::read(&catalog_path).expect("could not read catalog.json");
    let signature: Signature = signing_key.sign(&catalog_bytes);

    // Verify against ourselves before writing to disk so a corrupt PEM
    // or a stale buffer never produces an invalid .sig file.
    verifying_key
        .verify(&catalog_bytes, &signature)
        .expect("self-verify failed; refusing to write signature");

    let signature_b64 = BASE64.encode(signature.to_bytes());

    let sig_path = {
        let mut path = catalog_path.clone();
        path.set_extension("json.sig");
        path
    };
    fs::write(&sig_path, &signature_b64).expect("could not write signature");

    println!(
        "signed {} ({} bytes) -> {} ({} bytes)",
        catalog_path.display(),
        catalog_bytes.len(),
        sig_path.display(),
        signature_b64.len(),
    );
}
