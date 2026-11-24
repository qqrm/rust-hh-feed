//! Bind the published binary to its source commit and delivery protocol.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;

pub const PROTOCOL: u32 = 2;

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct DeliveryManifest {
    pub protocol: u32,
    pub sha256: String,
    pub source_sha: String,
}

fn is_hex(value: &str, length: usize) -> bool {
    value.len() == length && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn digest(binary: &Path) -> Result<String> {
    let mut file = File::open(binary)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn create(binary: &Path, manifest: &Path, source_sha: &str) -> Result<()> {
    if !is_hex(source_sha, 40) {
        bail!("Invalid release source commit");
    }
    let record = DeliveryManifest {
        protocol: PROTOCOL,
        sha256: digest(binary)?,
        source_sha: source_sha.to_owned(),
    };
    let mut file = File::create(manifest)?;
    serde_json::to_writer(&mut file, &record)?;
    writeln!(file)?;
    Ok(())
}

pub fn verify(binary: &Path, manifest: &Path) -> Result<()> {
    let contents = fs::read_to_string(manifest)?;
    let record: DeliveryManifest = serde_json::from_str(&contents)?;
    if record.protocol != PROTOCOL {
        bail!("Release does not support the required delivery checkpoint protocol");
    }
    if !is_hex(&record.source_sha, 40) {
        bail!("Invalid release source commit");
    }
    if !is_hex(&record.sha256, 64) || record.sha256 != digest(binary)? {
        bail!("Release binary and manifest do not match; wait for release completion");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn round_trip_and_failure_cases() {
        let dir = tempdir().unwrap();
        let binary = dir.path().join("bot");
        let manifest = dir.path().join("manifest.json");
        fs::write(&binary, b"binary-with-pending-protocol").unwrap();
        create(&binary, &manifest, &"a".repeat(40)).unwrap();
        verify(&binary, &manifest).unwrap();

        fs::write(&binary, b"old-or-different-binary").unwrap();
        assert!(verify(&binary, &manifest).is_err());

        let record: serde_json::Value =
            serde_json::from_slice(&fs::read(&manifest).unwrap()).unwrap();
        assert_eq!(record["protocol"], PROTOCOL);

        assert!(create(&binary, &manifest, "main").is_err());
    }
}
