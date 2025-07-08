use anyhow::Result;
use reqwest::Client;
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
        Self::with_base_url(token, chat_id, "https://api.telegram.org".into())
    }

    pub fn with_base_url(token: String, chat_id: String, base_url: String) -> Self {
        Self {
            token,
            chat_id,
            client: Client::new(),
            base_url,
        }
    }

    pub async fn send_message(&self, text: &str) -> Result<()> {
        let msg = Message {
            chat_id: &self.chat_id,
            text,
            parse_mode: "Markdown",
        };
        self.client
            .post(format!(
                "{}/bot{token}/sendMessage",
                self.base_url,
                token = self.token
            ))
            .json(&msg)
            .send()
            .await?;
        Ok(())
    }
}
