//! Metadata-only provenance checks for Dependabot publication batches.

use anyhow::{bail, Context, Result};
use reqwest::header::{ACCEPT, AUTHORIZATION, USER_AGENT};
use reqwest::Client;
use serde_json::Value;
use std::env;
use std::fs;
use std::time::Duration;

/// A batch qualifies only when every commit is a verified Dependabot commit and
/// the final commit still matches the checked event head.
pub fn verified_bot_commits(commits: &[Value], head: &str) -> bool {
    if commits.is_empty() || commits.last().and_then(|c| c["sha"].as_str()) != Some(head) {
        return false;
    }
    commits.iter().all(|commit| {
        commit["author"]["login"].as_str() == Some("dependabot[bot]")
            && commit["commit"]["verification"]["verified"].as_bool() == Some(true)
    })
}

pub async fn verify_dependabot_provenance() -> Result<()> {
    let event_path = env::var("GITHUB_EVENT_PATH").context("GITHUB_EVENT_PATH is not set")?;
    let event: Value = serde_json::from_slice(&fs::read(event_path)?)?;
    let number = event["pull_request"]["number"]
        .as_u64()
        .context("event has no pull request number")?;
    let repository = event["repository"]["full_name"]
        .as_str()
        .context("event has no repository name")?;
    let head = event["pull_request"]["head"]["sha"]
        .as_str()
        .context("event has no pull request head SHA")?;
    let token = env::var("GH_TOKEN").context("GH_TOKEN is not set")?;
    let api_url = env::var("GITHUB_API_URL")
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "https://api.github.com".to_owned());
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .context("failed to build policy HTTP client")?;

    let mut commits = Vec::new();
    let mut page = 1u32;
    loop {
        let url = format!(
            "{}/repos/{repository}/pulls/{number}/commits?per_page=100&page={page}",
            api_url.trim_end_matches('/')
        );
        let response = client
            .get(url)
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .header(ACCEPT, "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header(USER_AGENT, "rust-hh-feed/1.0")
            .send()
            .await
            .context("Dependabot provenance request failed")?
            .error_for_status()
            .context("Dependabot provenance request was rejected")?;
        let batch: Vec<Value> = response.json().await?;
        let last_page = batch.len() < 100;
        commits.extend(batch);
        if last_page {
            break;
        }
        page += 1;
    }

    if !verified_bot_commits(&commits, head) {
        bail!(
            "The checked batch must contain only verified Dependabot commits and still match the event's head commit."
        );
    }
    println!("Dependabot provenance verified. All update types use the required CI gates.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn commit(sha: &str, author: &str, verified: bool) -> Value {
        json!({
            "sha": sha,
            "author": {"login": author},
            "commit": {"verification": {"verified": verified}},
        })
    }

    #[test]
    fn signed_bot_batch_is_accepted() {
        let commits = vec![
            commit("first", "dependabot[bot]", true),
            commit("head", "dependabot[bot]", true),
        ];
        assert!(verified_bot_commits(&commits, "head"));
    }

    #[test]
    fn empty_changed_or_unsigned_batches_fail_closed() {
        assert!(!verified_bot_commits(&[], "head"));
        assert!(!verified_bot_commits(
            &[commit("new", "dependabot[bot]", true)],
            "head"
        ));
        assert!(!verified_bot_commits(
            &[commit("head", "contributor", true)],
            "head"
        ));
        assert!(!verified_bot_commits(
            &[commit("head", "dependabot[bot]", false)],
            "head"
        ));
        assert!(!verified_bot_commits(&[json!({"sha": "head"})], "head"));
    }
}
