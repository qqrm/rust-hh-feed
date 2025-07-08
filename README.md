# rust-hh-feed

This project collects daily job postings related to the Rust programming language from HeadHunter and posts them to a Telegram channel.

## Main Features

- Query the hh.ru API for fresh vacancies using the keyword "Rust".
- Filter vacancies where "Rust" appears in the title or key requirements.
- Publish the results to a Telegram channel via a bot.
- Schedule the process to run once per day.

## Components

1. **HeadHunter parser** — a Rust module that queries the API.
2. **Collector and filter** — processes vacancies and selects relevant ones.
3. **Telegram bot** — sends messages to the channel.
4. **Scheduler** — triggers the collection and posting.

## Documentation
- [Project architecture](docs/README.md)
- [Publication state storage](docs/TECHNICAL_DETAILS.md)

## Configuration
The bot expects a few environment variables:

| Variable | Purpose |
|----------|---------------------------------------------------------|
| `TELEGRAM_BOT_TOKEN` | Telegram bot token |
| `TELEGRAM_CHAT_ID` | ID of the channel to post jobs |
| `MANUAL_MODE` | Set to `true` to skip saving posted jobs |

Create a `.env` file using [`.env.example`](.env.example) as a template.

## License
This project is licensed under the [MIT](LICENSE) license.
