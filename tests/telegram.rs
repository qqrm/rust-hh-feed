use mockito::{mock, server_url};
use reqwest::StatusCode;
use rust_hh_feed::telegram::TelegramBot;

#[tokio::test]
async fn send_message_returns_status() {
    let token = "test".to_string();
    let chat_id = "123".to_string();
    let path = format!("/bot{token}/sendMessage");
    let _m = mock("POST", path.as_str())
        .with_status(200)
        .with_header("content-type", "application/json")
        .create();

    let bot = TelegramBot::with_base_url(token, chat_id, server_url());
    let status = bot.send_message("hi").await.unwrap();
    assert_eq!(status, StatusCode::OK);
}
