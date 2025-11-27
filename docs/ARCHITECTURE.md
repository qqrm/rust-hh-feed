# Project Architecture

This document describes the intended structure of the bot that searches for vacancies on HeadHunter and posts them to Telegram.

## Components

1. **hh_feed module**
   - Queries the hh.ru API with search terms like `rust`, `rust-разработчик`, `rust-developer`, `rust-programmer`, and `rust-программист`.
   - Selects vacancies where "Rust" appears in the title.
   - Extracts contact details and a job link when possible.
2. **Telegram module**
   - Uses the Bot API to send messages.
   - Stores the token and channel ID in the configuration.
3. **Scheduler**
   - Runs the update process every 30 minutes, typically via GitHub Actions.
   - Requests vacancies published in the last 45 minutes to avoid gaps when the pipeline is slow.
   - Removes completed workflow runs older than three days using the `cleanup-old-runs` job.

## Data Flow
1. The scheduler initiates the task.
2. The hh_feed module requests vacancies and filters the relevant ones.
3. The Telegram module publishes a message with the list of vacancies.

All modules are implemented in Rust. The configuration is expected in a `.env` file.
