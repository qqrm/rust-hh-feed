//! Durable pending/confirmed delivery state backed by the GitHub contents API.
//!
//! The remote state lives in a versioned JSON envelope on the `delivery-state`
//! branch. Every mutation compares the remote blob SHA with the receipt written
//! at restore time, so a stale runner can never replace newer progress.

use anyhow::{bail, Context, Result};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use reqwest::header::{ACCEPT, AUTHORIZATION, USER_AGENT};
use reqwest::{Client, Method, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

const BRANCH: &str = "delivery-state";
const BOOTSTRAP: &str = "confirmed-absent";
const DEFAULT_API_URL: &str = "https://api.github.com";
const ENVELOPE_VERSION: u32 = 1;

#[derive(Debug, Clone)]
pub struct Config {
    repository: String,
    token: String,
    key: String,
    api_url: String,
    client: Client,
}

impl Config {
    pub fn new(
        repository: impl Into<String>,
        token: impl Into<String>,
        key: impl Into<String>,
        api_url: impl Into<String>,
    ) -> Result<Self> {
        let key = key.into();
        if key.contains('/') || matches!(key.as_str(), "" | "." | "..") {
            bail!("Checkpoint key must be a filename");
        }
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .context("failed to build checkpoint HTTP client")?;
        Ok(Self {
            repository: repository.into(),
            token: token.into(),
            key,
            api_url: api_url.into(),
            client,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Save,
    Begin,
    Confirm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreOutcome {
    Restored,
    Bootstrap,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
struct Envelope {
    // Declaration order matches the legacy Python `sort_keys=True` envelope.
    pending: Option<String>,
    state: String,
    version: u32,
}

/// Read the checkpoint configuration. All three variables must be present
/// together; when none are set the caller runs in local-only mode.
pub fn config_from_env() -> Result<Option<Config>> {
    let repository = env::var("GITHUB_REPOSITORY").ok().filter(|v| !v.is_empty());
    let token = env::var("GH_TOKEN").ok().filter(|v| !v.is_empty());
    let key = env::var("DELIVERY_STATE_KEY")
        .ok()
        .filter(|v| !v.is_empty());

    match (repository, token, key) {
        (None, None, None) => Ok(None),
        (Some(repository), Some(token), Some(key)) => {
            let api_url = env::var("GITHUB_API_URL")
                .ok()
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| DEFAULT_API_URL.to_owned());
            Ok(Some(Config::new(repository, token, key, api_url)?))
        }
        _ => {
            bail!("Checkpoint requires GITHUB_REPOSITORY, GH_TOKEN and DELIVERY_STATE_KEY together")
        }
    }
}

fn contents_endpoint(config: &Config) -> String {
    let key: String = config
        .key
        .bytes()
        .flat_map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                vec![byte as char]
            }
            _ => format!("%{byte:02X}").chars().collect(),
        })
        .collect();
    format!("/contents/{key}?ref={BRANCH}")
}

async fn api(
    config: &Config,
    endpoint: &str,
    method: Method,
    payload: Option<Value>,
) -> Result<Option<Value>> {
    let is_get = method == Method::GET;
    let url = format!(
        "{}/repos/{}{}",
        config.api_url.trim_end_matches('/'),
        config.repository,
        endpoint
    );
    let mut request = config
        .client
        .request(method, url)
        .header(AUTHORIZATION, format!("Bearer {}", config.token))
        .header(ACCEPT, "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header(USER_AGENT, "rust-hh-feed/1.0");
    if let Some(payload) = payload {
        request = request.json(&payload);
    }

    let response = request
        .send()
        .await
        .context("Checkpoint API connection failed")?;
    if response.status() == StatusCode::NOT_FOUND && is_get {
        return Ok(None);
    }
    if !response.status().is_success() {
        bail!("Checkpoint API failed: HTTP {}", response.status());
    }
    let body = response
        .json::<Value>()
        .await
        .context("Invalid checkpoint API response")?;
    Ok(Some(body))
}

fn decode_envelope(remote: &Value) -> Result<Envelope> {
    let content = remote
        .get("content")
        .and_then(Value::as_str)
        .context("Checkpoint response did not contain content")?;
    let raw: String = content.chars().filter(|c| !c.is_whitespace()).collect();
    let decoded = STANDARD
        .decode(raw)
        .context("Invalid delivery checkpoint envelope encoding")?;
    let envelope: Envelope =
        serde_json::from_slice(&decoded).context("Invalid delivery checkpoint envelope")?;
    if envelope.version != ENVELOPE_VERSION {
        bail!("Invalid delivery checkpoint envelope");
    }
    STANDARD
        .decode(&envelope.state)
        .context("Invalid delivery checkpoint state encoding")?;
    Ok(envelope)
}

fn expected_path(local_file: &Path) -> PathBuf {
    PathBuf::from(format!("{}.remote-sha", local_file.display()))
}

fn remember(local_file: &Path, sha: &str) -> Result<()> {
    if sha.is_empty() {
        bail!("Checkpoint response did not identify the committed blob");
    }
    let path = expected_path(local_file);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = PathBuf::from(format!("{}.tmp", path.display()));
    fs::write(&temporary, sha)?;
    fs::rename(&temporary, &path)?;
    Ok(())
}

fn read_receipt(local_file: &Path) -> Result<String> {
    let sha = fs::read_to_string(expected_path(local_file))
        .context("Missing delivery checkpoint restore receipt")?;
    if sha.is_empty() {
        bail!("Empty delivery checkpoint restore receipt");
    }
    Ok(sha)
}

fn write_local(local_file: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = local_file.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = local_file.with_extension("checkpoint-tmp");
    fs::write(&temporary, bytes)?;
    fs::rename(&temporary, local_file)?;
    Ok(())
}

fn remote_sha(remote: &Value) -> Result<&str> {
    remote
        .get("sha")
        .and_then(Value::as_str)
        .context("Checkpoint response did not identify the committed blob")
}

fn hex_digest(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

/// Restore the confirmed state. A missing checkpoint is only allowed when the
/// branch itself does not exist yet.
pub async fn restore(config: &Config, local_file: &Path) -> Result<RestoreOutcome> {
    let endpoint = contents_endpoint(config);
    let remote = api(config, &endpoint, Method::GET, None).await?;
    let envelope = match &remote {
        Some(remote) => Some(decode_envelope(remote)?),
        None => None,
    };
    if envelope
        .as_ref()
        .is_some_and(|envelope| envelope.pending.is_some())
    {
        bail!("Unconfirmed delivery pending; reconcile delivery-state before retrying");
    }

    let Some(remote) = remote else {
        if api(
            config,
            &format!("/git/ref/heads/{BRANCH}"),
            Method::GET,
            None,
        )
        .await?
        .is_some()
        {
            bail!("Existing delivery-state branch has no checkpoint");
        }
        if api(config, "", Method::GET, None).await?.is_none() {
            bail!("Repository unavailable; cannot establish first state");
        }
        remember(local_file, BOOTSTRAP)?;
        return Ok(RestoreOutcome::Bootstrap);
    };

    let state = STANDARD.decode(&envelope.expect("checked above").state)?;
    write_local(local_file, &state)?;
    remember(local_file, remote_sha(&remote)?)?;
    Ok(RestoreOutcome::Restored)
}

/// Commit a local state change. Every mutation is bound to the blob SHA seen
/// at restore time (or at the previous successful write).
pub async fn commit(
    config: &Config,
    local_file: &Path,
    operation: Operation,
    identity: Option<&str>,
) -> Result<()> {
    let endpoint = contents_endpoint(config);
    let remote = api(config, &endpoint, Method::GET, None).await?;
    let envelope = match &remote {
        Some(remote) => Some(decode_envelope(remote)?),
        None => None,
    };
    if operation != Operation::Confirm
        && envelope
            .as_ref()
            .is_some_and(|envelope| envelope.pending.is_some())
    {
        bail!("Unconfirmed delivery pending; reconcile delivery-state before retrying");
    }

    let expected = read_receipt(local_file)?;
    match &remote {
        Some(remote) if remote_sha(remote)? != expected => {
            bail!("Delivery checkpoint changed since restore; refusing stale state")
        }
        None if expected != BOOTSTRAP => {
            bail!("Delivery checkpoint changed since restore; refusing stale state")
        }
        _ => {}
    }
    if matches!(operation, Operation::Begin | Operation::Confirm) && envelope.is_none() {
        bail!("Delivery must establish initial state before sending");
    }
    if operation == Operation::Confirm
        && envelope
            .as_ref()
            .is_some_and(|envelope| envelope.pending.is_none())
    {
        bail!("No pending delivery to confirm");
    }

    if remote.is_none() {
        if api(
            config,
            &format!("/git/ref/heads/{BRANCH}"),
            Method::GET,
            None,
        )
        .await?
        .is_some()
        {
            bail!("Existing delivery-state branch has no checkpoint");
        }
        let repository = api(config, "", Method::GET, None)
            .await?
            .context("Repository unavailable; cannot establish initial state")?;
        let default_branch = repository["default_branch"]
            .as_str()
            .context("Repository response has no default branch")?;
        let head = api(
            config,
            &format!("/git/ref/heads/{default_branch}"),
            Method::GET,
            None,
        )
        .await?
        .context("Default branch reference is missing")?;
        let head_sha = head["object"]["sha"]
            .as_str()
            .context("Default branch reference has no object SHA")?;
        api(
            config,
            "/git/refs",
            Method::POST,
            Some(json!({
                "ref": format!("refs/heads/{BRANCH}"),
                "sha": head_sha,
            })),
        )
        .await?;
    }

    let bytes = fs::read(local_file).context("failed to read local delivery state")?;
    let next = Envelope {
        version: ENVELOPE_VERSION,
        state: STANDARD.encode(bytes),
        pending: match operation {
            Operation::Begin => Some(hex_digest(
                identity.context("Begin requires a delivery identity")?,
            )),
            _ => None,
        },
    };
    if envelope.as_ref() == Some(&next) {
        return Ok(());
    }

    let mut payload = json!({
        "message": "Checkpoint delivery progress",
        "branch": BRANCH,
        "content": STANDARD.encode(serde_json::to_vec(&next)?),
    });
    if let Some(remote) = &remote {
        payload["sha"] = json!(remote_sha(remote)?);
    }
    let response = api(config, &endpoint, Method::PUT, Some(payload))
        .await?
        .context("Checkpoint write returned no response")?;
    let sha = response
        .get("content")
        .and_then(|content| content.get("sha"))
        .and_then(Value::as_str)
        .context("Checkpoint response did not identify the committed blob")?;
    remember(local_file, sha)?;
    Ok(())
}

/// The marker written next to the local state before an externally visible send.
pub fn pending_path(local_file: &Path) -> PathBuf {
    local_file.with_extension("pending")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_decoding_rejects_bad_versions_and_encodings() {
        let valid = json!({
            "content": STANDARD.encode(
                json!({"version": 1, "state": STANDARD.encode(b"confirmed"), "pending": null})
                    .to_string()
            ),
        });
        assert_eq!(
            decode_envelope(&valid).unwrap().state,
            STANDARD.encode(b"confirmed")
        );

        let bad_version = json!({
            "content": STANDARD.encode(r#"{"version":2,"state":"YQ==","pending":null}"#),
        });
        assert!(decode_envelope(&bad_version).is_err());

        let bad_state = json!({
            "content": STANDARD.encode(r#"{"version":1,"state":"!!!","pending":null}"#),
        });
        assert!(decode_envelope(&bad_state).is_err());
    }

    #[test]
    fn expected_path_matches_legacy_receipt_name() {
        assert_eq!(
            expected_path(Path::new("data/posted_jobs.json")),
            PathBuf::from("data/posted_jobs.json.remote-sha")
        );
    }
}
