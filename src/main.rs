use rust_hh_feed::hh;
use rust_hh_feed::state;
use rust_hh_feed::telegram;

use anyhow::Context;
use chrono::Utc;
use state::{load_posted_jobs, save_posted_jobs};
use std::path::Path;
use telegram::TelegramBot;

/// Environment variable that enables manual mode.
const MANUAL_MODE_VAR: &str = "MANUAL_MODE";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    let hh_client = if let Ok(url) = std::env::var("HH_BASE_URL") {
        hh::HhClient::with_base_url(url)
    } else {
        hh::HhClient::new()
    };
    let jobs = hh_client.fetch_jobs().await?;

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

    for job in &jobs {
        bot.send_message(&job.url).await?;
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }

    let path = std::env::var("POSTED_JOBS_PATH").unwrap_or_else(|_| "data/posted_jobs.json".into());
    let mut posted = load_posted_jobs(Path::new(&path))?;
    for job in jobs {
        posted
            .entry(job.id)
            .or_insert_with(|| Utc::now().date_naive().to_string());
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
