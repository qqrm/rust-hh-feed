# rust-hh-feed

This project collects daily job postings related to the Rust programming language from HeadHunter and posts them to a Telegram channel.
You can join the Telegram channel at [RustHH Jobs](https://t.me/rusthhjobs).

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
|----------|--------------------------------------------------------------|
| `TELEGRAM_BOT_TOKEN` | Telegram bot token |
| `TELEGRAM_CHAT_ID` | ID of the channel to post jobs |
| `DEV_TELEGRAM_CHAT_ID` | ID of the development channel used in CI |
| `HH_BASE_URL` | Override base URL for the HeadHunter API |
| `TELEGRAM_API_BASE_URL` | Override base URL for the Telegram Bot API |
| `POSTED_JOBS_PATH` | Path to the JSON file with already posted jobs |
| `MANUAL_MODE` | Set to `true` to skip saving posted jobs |
| `RUN_INTEGRATION` | Set to `true` to run the bot during CI |

Create a `.env` file using [`.env.example`](.env.example) as a template.

## Quiet CI Logs
When running CI workflows you can suppress crate download and compilation
messages by adding `--quiet` to the Cargo commands. For example:

```
cargo clippy --quiet --all-targets --all-features -- -D warnings
cargo test --quiet
cargo run --release --quiet
```

This keeps the logs short while still printing warnings and errors.

## Continuous Integration
Pull requests trigger the `ci.yml` workflow that checks formatting,
lint rules, `cargo machete`, and tests. Workflows `daily_post.yml` and
`manual_post.yml` only build and run the application. After `pr_checks.yml` succeeds, the `auto_merge.yml` workflow enables pull request auto-merge.

## License
This project is licensed under the [MIT](LICENSE) license.
