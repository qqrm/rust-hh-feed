# rust-hh-feed

This project collects hourly job postings related to the Rust programming language from HeadHunter and posts them to a Telegram channel.
You can join the Telegram channel at [RustHH Jobs](https://t.me/rusthhjobs).

## Main Features

- Query the hh.ru API for fresh vacancies using the keyword "Rust".
- Filter vacancies where "Rust" appears in the title or key requirements.
- Publish the results to a Telegram channel via a bot.
- Schedule the process to run every hour.

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
| `JOB_RETENTION_DAYS` | Maximum age in days to keep posted job IDs |

The file referenced by `POSTED_JOBS_PATH` is not committed to the repository. It is downloaded from the previous workflow run and uploaded back as an artifact after each execution.

During continuous integration the bot posts to the development channel
configured by `DEV_TELEGRAM_CHAT_ID`. Scheduled runs and manual releases use
`TELEGRAM_CHAT_ID` to post to the production channel.

Set the `RUST_LOG` environment variable to control the logging level, for
example `RUST_LOG=info`.

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
Pull requests trigger the [`ci.yml`](.github/workflows/ci.yml) workflow that checks formatting,
lint rules, `cargo machete`, and tests. The `post.yml` workflow
builds and runs the application either on schedule or manually. After
`ci.yml` succeeds, the `auto_merge.yml` workflow merges the pull request using the `gh` CLI.
Only pull requests opened by the `qqrm` account trigger these workflows.
Every workflow starts with a job that verifies the author and fails if the pull
request is not from `qqrm`.

The CI job caches Cargo dependencies and build artifacts to speed up subsequent
runs. For each update to the `main` branch the same workflow uploads the latest
compiled binary to the [`latest`](../../releases/latest) release. You can also
download artifacts directly from the workflow run page.

Additional workflows automate repository maintenance:

- `pr_cleanup.yml` cancels running CI jobs and deletes the branch after a pull request is merged while skipping its own run.
- `manual_release.yml` allows manual execution of the bot through the GitHub UI.
 - The `cleanup-old-runs` job inside `post.yml` deletes completed runs of all workflows after three days using `GITHUB_TOKEN` with the `actions: write` permission.


## Release Binary
A push to the `main` branch updates the `latest` release with a freshly built executable. Only one file, `rust-hh-feed`, is kept in the release. Download it with:

```
curl -L https://github.com/<owner>/<repo>/releases/latest/download/rust-hh-feed -o rust-hh-feed
```

## License
This project is licensed under the [MIT](LICENSE) license.
