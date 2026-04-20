# rust-hh-feed

This project collects job postings related to the Rust programming language from HeadHunter and posts them to a Telegram channel.
You can join the Telegram channel at [RustHH Jobs](https://t.me/rusthhjobs).

## Main Features

- Read the HeadHunter search RSS feed for fresh `rust` vacancies ordered by publication time.
- Filter vacancies where "Rust" appears in the title.
- Publish the results to a Telegram channel via a bot.
- Run the posting pipeline from GitHub Actions when the HeadHunter workflows are enabled.

## Components

1. **HeadHunter parser** — a Rust module that reads the HeadHunter search RSS feed.
2. **Collector and filter** — processes vacancies and selects relevant ones.
3. **Telegram bot** — sends messages to the channel.
4. **Scheduler** — triggers the collection and posting when the HeadHunter workflows are enabled.

## Documentation
- [Project architecture](docs/README.md)
- [Publication state storage](docs/TECHNICAL_DETAILS.md)

## Configuration
The bot expects a few environment variables:

| Variable | Purpose |
|----------|--------------------------------------------------------------|
| `TELEGRAM_BOT_TOKEN` | Telegram bot token |
| `TELEGRAM_CHAT_ID` | ID of the channel to post jobs |
| `HH_BASE_URL` | Override base URL for the HeadHunter web host or a test double |
| `HH_USER_AGENT` | Override the `User-Agent` value sent to HeadHunter search requests |
| `HH_PROXY_URLS` | Optional comma- or newline-separated proxy URLs for HeadHunter requests only |
| `HH_PROXY_SOURCE_URLS` | Optional comma- or newline-separated URLs that return proxy candidates; if unset, the bot polls built-in public proxy providers |
| `HH_PROXY_PROBE_TIMEOUT_SECS` | Timeout for fetching and probing HeadHunter proxy candidates |
| `BACKFILL_HOURS` | Optional one-off override that widens the fetch window for backfills |
| `TELEGRAM_API_BASE_URL` | Override base URL for the Telegram Bot API |
| `POSTED_JOBS_PATH` | Path to the JSON file with already posted jobs |
| `MANUAL_MODE` | Set to `true` to skip saving posted jobs |
| `JOB_RETENTION_DAYS` | Maximum age in days to keep posted job IDs |

The file referenced by `POSTED_JOBS_PATH` is not committed to the repository. It is downloaded from the previous successful workflow run and uploaded back as an artifact only after a new successful execution. The state file also stores the timestamp of the last committed run so the bot can re-fetch vacancies after one or more failed runs.
The collector now uses the HeadHunter search RSS feed exposed by the public vacancy search page instead of the blocked `/vacancies` API endpoint. `HH_USER_AGENT` remains configurable so production runs can identify themselves consistently.
The bot can dynamically poll several built-in public proxy providers for Russian free proxies when `HH_PROXY_SOURCE_URLS` is unset. If you do define `HH_PROXY_SOURCE_URLS`, those custom sources replace the built-in provider list.
If `HH_PROXY_URLS` is configured or a proxy source returns candidates, the bot probes a bounded candidate set against the HeadHunter search RSS endpoint, keeps the working proxies in order, and retries the real feed request through them until one succeeds before falling back to direct access. Telegram traffic always stays on the direct network path.
For one-off recovery runs you can set `BACKFILL_HOURS`, typically `72`, to fetch missed vacancies from the last three days plus the normal overlap in addition to the state-based window.
The bot always fetches from the last successful committed run with a small overlap window. If the state file is missing or does not contain a committed timestamp yet, it falls back to a wider bootstrap window to reduce the chance of missing vacancies during unstable scheduling.

During continuous integration the workflow sets `TELEGRAM_CHAT_ID` to a development channel.
Scheduled runs and manual releases use the production chat ID.
The HeadHunter posting workflows run on a 20-minute schedule when the GitHub Actions variable `HH_PIPELINES_ENABLED=true`.

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
can run the application manually when the HeadHunter workflows are enabled. After
`ci.yml` succeeds, the `auto_merge.yml` workflow merges the pull request using the `gh` CLI.
Dependabot now manages three update surfaces:

- Cargo crates from `Cargo.toml` and `Cargo.lock`
- GitHub Actions used in `.github/workflows/`
- Rust toolchain versions from `rust-toolchain.toml`

Internal pull requests are merged by `auto_merge.yml` only after their own
`CI Checks` workflow completes successfully. The allowlist is limited to
same-repository PRs from `dependabot[bot]`, `qqrm`, and `codex-bot`, so
external contributor branches are not auto-merged. GitHub Actions updates are
grouped into a single weekly pull request to reduce merge churn, and
third-party actions in the workflows are pinned to immutable commit SHAs. A
push to `main` then triggers `release.yml`, which rebuilds the production
binary and updates the [`latest`](../../releases/latest) release through the
GitHub CLI when the bot sources or build inputs change on `main`. The scheduled
posting pipeline downloads that release asset instead of rebuilding the bot on
every run.

The CI job caches Cargo dependencies and build artifacts to speed up subsequent
runs. For each update to the `main` branch the same workflow uploads the latest
compiled binary to the [`latest`](../../releases/latest) release. You can also
download artifacts directly from the workflow run page.

Additional workflows automate repository maintenance:

- `pr_cleanup.yml` cancels running CI jobs and deletes the branch after a pull request is merged while skipping its own run.
- `manual_release.yml` allows manual execution of the bot through the GitHub UI when `HH_PIPELINES_ENABLED=true`.
- The `cleanup-old-runs` job inside `post.yml` deletes completed runs of all workflows after three days using `GITHUB_TOKEN` with the `actions: write` permission.


## Release Binary
A push to the `main` branch updates the `latest` release with a freshly built executable. Only one file, `rust-hh-feed`, is kept in the release. Download it with:

```
curl -L https://github.com/<owner>/<repo>/releases/latest/download/rust-hh-feed -o rust-hh-feed
```

## License
This project is licensed under the [MIT](LICENSE) license.
