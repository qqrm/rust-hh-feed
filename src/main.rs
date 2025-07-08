mod hh;
mod state;
mod telegram;

use chrono::Utc;
use state::{load_posted_jobs, save_posted_jobs};
use std::path::Path;
use telegram::TelegramBot;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let hh_client = hh::HhClient::new();
    let jobs = hh_client.fetch_jobs().await?;

    let token = std::env::var("TELOXIDE_TOKEN").unwrap_or_default();
    let raw_chat_id = std::env::var("TELEGRAM_CHAT_ID").unwrap_or_default();
    let chat_id = if raw_chat_id.starts_with("-100") {
        raw_chat_id
    } else {
        format!("-100{raw_chat_id}")
    };
    let bot = TelegramBot::new(token, chat_id);

    let message = format!("Found {jobs_len} Rust jobs", jobs_len = jobs.len());
    bot.send_message(&message).await?;

    let mut posted = load_posted_jobs(Path::new("data/posted_jobs.json"))?;
    for job in jobs {
        posted
            .entry(job.id)
            .or_insert_with(|| Utc::now().date_naive().to_string());
    }
    save_posted_jobs(Path::new("data/posted_jobs.json"), &posted)?;

    Ok(())
}
