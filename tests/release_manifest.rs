//! The release manifest must fail closed on every mismatch.

use rust_hh_feed::release_manifest::{create, verify, PROTOCOL};
use serde_json::{json, Value};
use std::fs;
use tempfile::tempdir;

fn fixture() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let dir = tempdir().unwrap();
    let binary = dir.path().join("rust-hh-feed");
    let manifest = dir.path().join("delivery-manifest.json");
    fs::write(&binary, b"binary-with-pending-protocol").unwrap();
    create(&binary, &manifest, &"a".repeat(40)).unwrap();
    (dir, binary, manifest)
}

#[test]
fn matching_protocol_and_hash_pass() {
    let (_dir, binary, manifest) = fixture();
    verify(&binary, &manifest).unwrap();
}

#[test]
fn old_binary_or_racing_asset_update_fails() {
    let (_dir, binary, manifest) = fixture();
    fs::write(&binary, b"old-or-different-binary").unwrap();
    assert!(verify(&binary, &manifest).is_err());
}

#[test]
fn manifest_records_the_rust_protocol() {
    let (_dir, _binary, manifest) = fixture();
    let record: Value = serde_json::from_slice(&fs::read(&manifest).unwrap()).unwrap();
    assert_eq!(record["protocol"], PROTOCOL);
    assert_eq!(record["source_sha"], "a".repeat(40));
    assert_eq!(record["sha256"].as_str().unwrap().len(), 64);
}

#[test]
fn unsupported_protocol_fails() {
    let (_dir, binary, manifest) = fixture();
    fs::write(
        &manifest,
        json!({
            "protocol": 0,
            "sha256": "0".repeat(64),
            "source_sha": "a".repeat(40),
        })
        .to_string(),
    )
    .unwrap();
    assert!(verify(&binary, &manifest).is_err());
}

#[test]
fn missing_manifest_fails() {
    let (_dir, binary, manifest) = fixture();
    fs::remove_file(&manifest).unwrap();
    assert!(verify(&binary, &manifest).is_err());
}

#[test]
fn malformed_manifest_fails() {
    let (_dir, binary, manifest) = fixture();
    fs::write(&manifest, "{}").unwrap();
    assert!(verify(&binary, &manifest).is_err());
}

#[test]
fn invalid_source_commit_fails() {
    let (_dir, binary, manifest) = fixture();
    assert!(create(&binary, &manifest, "main").is_err());
}
