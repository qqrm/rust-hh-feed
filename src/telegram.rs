use anyhow::Result;
use reqwest::{Client, StatusCode};
use serde::Serialize;

pub struct TelegramBot {
    token: String,
    chat_id: String,
    client: Client,
    base_url: String,
}

#[derive(Serialize)]
struct Message<'a> {
    chat_id: &'a str,
    text: &'a str,
    parse_mode: &'a str,
}

impl TelegramBot {
    pub fn new(token: String, chat_id: String) -> Self {
        Self::with_base_url(token, chat_id, "https://api.telegram.org")
    }

    pub fn with_base_url(token: String, chat_id: String, base_url: impl Into<String>) -> Self {
        Self {
            token,
            chat_id,
            client: Client::new(),
            base_url: base_url.into(),
        }
    }

    pub async fn send_message(&self, text: &str) -> Result<StatusCode> {
        let msg = Message {
            chat_id: &self.chat_id,
            text,
            parse_mode: "Markdown",
        };
        let resp = self
            .client
            .post(format!(
                "{}/bot{token}/sendMessage",
                self.base_url,
                token = self.token
            ))
            .json(&msg)
            .send()
            .await?;
        Ok(resp.status())
    }
}
