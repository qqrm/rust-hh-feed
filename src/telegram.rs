use anyhow::Result;
use reqwest::Client;
use serde::Serialize;

pub struct TelegramBot {
    token: String,
    chat_id: String,
    client: Client,
}

#[derive(Serialize)]
struct Message<'a> {
    chat_id: &'a str,
    text: &'a str,
    parse_mode: &'a str,
}

impl TelegramBot {
    pub fn new(token: String, chat_id: String) -> Self {
        Self {
            token,
            chat_id,
            client: Client::new(),
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
                "https://api.telegram.org/bot{token}/sendMessage",
                token = self.token
            ))
            .json(&msg)
            .send()
            .await?;
        Ok(())
    }
}
