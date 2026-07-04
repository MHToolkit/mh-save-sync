use base64::Engine;
use clap::{Parser, Subcommand};
use ed25519_dalek::SigningKey;
use save_crypto::{
    account_handle, account_root_signing_key, derive_account_keys, deterministic_cbor,
    issue_device_certificate_with_id, recovery_phrase_from_secret,
};
use save_domain::{GameKey, stable_logical_save_id};
use save_engine::{SnapshotOptions, create_snapshot_from_stable_folder, decrypt_manifest};
use sha2::Digest;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "mh-save", about = "MH Save Sync research/phase1 CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Adapters,
    CryptoVector,
    CryptoDeviceFixture,
    SnapshotFixture { root: PathBuf },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Adapters => {
            let descriptors = save_adapters::all_descriptors();
            println!("{}", serde_json::to_string_pretty(&descriptors)?);
        }
        Commands::CryptoVector => {
            let secret = [0x42u8; 32];
            let keys = derive_account_keys(&secret)?;
            let phrase = recovery_phrase_from_secret(&secret)?;
            println!(
                "{{\"suite\":1,\"secret_sha256\":\"{}\",\"account_handle\":\"{}\",\"phrase_word_count\":{}}}",
                hex::encode(sha2::Sha256::digest(secret)),
                account_handle(&keys),
                phrase.split_whitespace().count()
            );
        }
        Commands::CryptoDeviceFixture => {
            let keys = derive_account_keys(&[0x42; 32])?;
            let root = account_root_signing_key(&keys);
            let device = SigningKey::from_bytes(&[0x24; 32]);
            let cert_id = [0x33; 16];
            let certificate = issue_device_certificate_with_id(
                &root,
                &device.verifying_key(),
                cert_id,
                1_700_000_000,
                4_102_444_800,
                1,
            )?;
            println!(
                "{}",
                serde_json::json!({
                    "account_handle": account_handle(&keys),
                    "root_public_key_b64": base64::engine::general_purpose::STANDARD
                        .encode(root.verifying_key().to_bytes()),
                    "cert_id": hex::encode(cert_id),
                    "device_public_key_b64": base64::engine::general_purpose::STANDARD
                        .encode(device.verifying_key().to_bytes()),
                    "certificate_b64": base64::engine::general_purpose::STANDARD
                        .encode(deterministic_cbor(&certificate)?),
                })
            );
        }
        Commands::SnapshotFixture { root } => {
            let descriptor = save_adapters::generic_folder_macos();
            let game_key = GameKey::new("generic", "fixture", "none", "slot1");
            let mut options = SnapshotOptions::fixture(game_key.clone());
            options.logical_save_id = stable_logical_save_id(&descriptor.emulator_id, &game_key);
            let secret = [0x11u8; 32];
            let snapshot =
                create_snapshot_from_stable_folder(&root, &descriptor, &secret, options)?;
            let manifest = decrypt_manifest(&secret, &snapshot)?;
            println!(
                "{}",
                serde_json::json!({
                    "snapshot_id": snapshot.snapshot_id,
                    "file_count": snapshot.fingerprint.file_count,
                    "total_bytes": snapshot.fingerprint.total_bytes,
                    "manifest_entries": manifest.entries.len(),
                    "chunk_count": snapshot.chunks.len()
                })
            );
        }
    }
    Ok(())
}
