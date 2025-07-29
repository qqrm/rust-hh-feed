use rust_hh_feed::hh;
use rust_hh_feed::state;
use rust_hh_feed::telegram;

use anyhow::Context;
use chrono::Utc;
use state::{load_posted_jobs, prune_old_jobs, save_posted_jobs};
use std::path::Path;
use telegram::TelegramBot;

/// Environment variable that enables manual mode.
const MANUAL_MODE_VAR: &str = "MANUAL_MODE";
/// Environment variable that sets how many days to keep posted IDs.
const JOB_RETENTION_VAR: &str = "JOB_RETENTION_DAYS";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    let hh_client = if let Ok(url) = std::env::var("HH_BASE_URL") {
        hh::HhClient::with_base_url(url)
    } else {
        hh::HhClient::new()
    };
    let jobs = hh_client.fetch_jobs().await?;

    let path = std::env::var("POSTED_JOBS_PATH").unwrap_or_else(|_| "data/posted_jobs.json".into());
    let mut posted = load_posted_jobs(Path::new(&path))?;
    let retention_days = std::env::var(JOB_RETENTION_VAR)
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(30);
    prune_old_jobs(&mut posted, retention_days);

    let new_jobs: Vec<_> = jobs
        .into_iter()
        .filter(|job| !posted.contains_key(&job.id))
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
            bot.send_message(&job.url).await?;
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }

        for job in &new_jobs {
            posted
                .entry(job.id.clone())
                .or_insert_with(|| Utc::now().date_naive().to_string());
        }
    }

    let manual_mode = std::env::var(MANUAL_MODE_VAR)
        .map(|v| v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    if !manual_mode {
        save_posted_jobs(Path::new(&path), &posted)?;
    } else {
        println!("Manual mode enabled - not saving state");
    }

    Ok(())
}
