use rust_hh_feed::checkpoint;
use rust_hh_feed::delivery_state::{self, Operation, RestoreOutcome};
use rust_hh_feed::hh;
use rust_hh_feed::policy;
use rust_hh_feed::release_manifest;
use rust_hh_feed::state;
use rust_hh_feed::telegram;

use anyhow::{bail, Context};
use chrono::{Duration, Utc};
use state::{load_posted_jobs, prune_old_jobs, resolve_fetch_window_start, save_posted_jobs};
use std::path::Path;
use telegram::TelegramBot;

/// Environment variable that enables manual mode.
const MANUAL_MODE_VAR: &str = "MANUAL_MODE";
/// Environment variable that sets how many days to keep posted IDs.
const JOB_RETENTION_VAR: &str = "JOB_RETENTION_DAYS";
/// Environment variable that widens the fetch window for one-off backfills.
const BACKFILL_HOURS_VAR: &str = "BACKFILL_HOURS";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        None => run_bot().await,
        Some("checkpoint") => checkpoint_cli(&args[1..]).await,
        Some("release-manifest") => manifest_cli(&args[1..]),
        Some("dependency-policy") => policy_cli(&args[1..]).await,
        Some(other) => bail!("Unknown command: {other}"),
    }
}

async fn checkpoint_cli(args: &[String]) -> anyhow::Result<()> {
    let config = delivery_state::config_from_env()?.context(
        "Checkpoint commands require GITHUB_REPOSITORY, GH_TOKEN and DELIVERY_STATE_KEY",
    )?;
    match args {
        [operation, file] if operation == "restore" => {
            match delivery_state::restore(&config, Path::new(file)).await? {
                RestoreOutcome::Restored => Ok(()),
                RestoreOutcome::Bootstrap => std::process::exit(3),
            }
        }
        [operation, file] if operation == "save" => {
            delivery_state::commit(&config, Path::new(file), Operation::Save, None).await
        }
        [operation, file] if operation == "confirm" => {
            delivery_state::commit(&config, Path::new(file), Operation::Confirm, None).await
        }
        [operation, file, identity] if operation == "begin" => {
            delivery_state::commit(&config, Path::new(file), Operation::Begin, Some(identity)).await
        }
        _ => bail!("Usage: rust-hh-feed checkpoint <restore|save|begin|confirm> <file> [identity]"),
    }
}

fn manifest_cli(args: &[String]) -> anyhow::Result<()> {
    match args {
        [operation, binary, manifest] if operation == "create" => {
            let source_sha = std::env::var("GITHUB_SHA").context("GITHUB_SHA is not set")?;
            release_manifest::create(Path::new(binary), Path::new(manifest), &source_sha)
        }
        [operation, binary, manifest] if operation == "verify" => {
            release_manifest::verify(Path::new(binary), Path::new(manifest))
        }
        _ => bail!("Usage: rust-hh-feed release-manifest <create|verify> <binary> <manifest>"),
    }
}

async fn policy_cli(args: &[String]) -> anyhow::Result<()> {
    match args {
        [operation] if operation == "verify" => policy::verify_dependabot_provenance().await,
        _ => bail!("Usage: rust-hh-feed dependency-policy verify"),
    }
}

async fn run_bot() -> anyhow::Result<()> {
    let hh_client = if let Ok(url) = std::env::var("HH_BASE_URL") {
        hh::HhClient::from_env_with_base_url(url).await?
    } else {
        hh::HhClient::from_env().await?
    };

    let path = std::env::var("POSTED_JOBS_PATH").unwrap_or_else(|_| "data/posted_jobs.json".into());
    let mut state = load_posted_jobs(Path::new(&path))?;
    let manual_mode = std::env::var(MANUAL_MODE_VAR)
        .map(|v| v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    if !manual_mode {
        // Establish a durable checkpoint before the first side effect.
        save_posted_jobs(Path::new(&path), &state).await?;
    }
    let retention_days = std::env::var(JOB_RETENTION_VAR)
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(30);
    prune_old_jobs(&mut state.jobs, retention_days);
    let now = Utc::now();
    let backfill_hours = std::env::var(BACKFILL_HOURS_VAR)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(|value| {
            value.parse::<i64>().with_context(|| {
                format!("{BACKFILL_HOURS_VAR} must be a positive integer number of hours")
            })
        })
        .transpose()?
        .map(|hours| {
            anyhow::ensure!(
                hours > 0,
                "{BACKFILL_HOURS_VAR} must be a positive integer number of hours"
            );
            Ok(hours)
        })
        .transpose()?;
    if let Some(last_successful_run_at) = state.last_successful_run_at {
        let gap = now - last_successful_run_at;
        if gap > Duration::minutes(20) {
            log::warn!(
                "Detected an unstable schedule gap: last successful committed run was {} minute(s) ago at {}",
                gap.num_minutes(),
                last_successful_run_at.to_rfc3339()
            );
        }
    } else {
        log::warn!(
            "No committed successful run timestamp found, falling back to the bootstrap fetch window"
        );
    }
    let fetch_from = resolve_fetch_window_start(state.last_successful_run_at, now, backfill_hours);
    if let Some(hours) = backfill_hours {
        log::warn!(
            "Backfill mode enabled, widening fetch window to the last {} hour(s)",
            hours
        );
    }
    log::info!(
        "Fetching jobs between {} and {} based on the last successful committed run with overlap",
        fetch_from.to_rfc3339(),
        now.to_rfc3339()
    );
    let jobs = hh_client.fetch_jobs_between(fetch_from, now).await?;

    let new_jobs: Vec<_> = jobs
        .into_iter()
        .filter(|job| job.name.to_lowercase().contains("rust"))
        .filter(|job| !state.jobs.contains_key(&job.id))
        .collect();
    log::info!("Found {} new job(s)", new_jobs.len());

    let token = std::env::var("TELEGRAM_BOT_TOKEN")
        .context("TELEGRAM_BOT_TOKEN environment variable not set")?;
    let raw_chat_id = std::env::var("TELEGRAM_CHAT_ID")
        .context("TELEGRAM_CHAT_ID environment variable not set")?;
    let chat_id = if raw_chat_id.starts_with("-100") {
        raw_chat_id
    } else {
        format!("-100{raw_chat_id}")
    };
    let bot = if let Ok(url) = std::env::var("TELEGRAM_API_BASE_URL") {
        TelegramBot::with_base_url(token, chat_id, url)
    } else {
        TelegramBot::new(token, chat_id)
    };

    if new_jobs.is_empty() {
        log::info!("No new jobs to post");
    } else {
        for job in &new_jobs {
            if !manual_mode {
                checkpoint::begin(Path::new(&path), &job.id).await?;
            }
            bot.send_message(&job.url).await?;
            state
                .jobs
                .entry(job.id.clone())
                .or_insert_with(|| Utc::now().date_naive().to_string());
            if !manual_mode {
                // Keep the fetch watermark unchanged until the whole batch succeeds.
                state::confirm_posted_jobs(Path::new(&path), &state).await?;
            }
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
    }

    if !manual_mode {
        state.last_successful_run_at = Some(now);
        save_posted_jobs(Path::new(&path), &state).await?;
    } else {
        println!("Manual mode enabled - not saving state");
    }

    Ok(())
}
