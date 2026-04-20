# Project Architecture

This document describes the intended structure of the bot that searches for vacancies on HeadHunter and posts them to Telegram.

## Components

1. **hh_feed module**
   - Reads the HeadHunter vacancy search RSS feed with `text=rust`, `search_field=name`, and `order_by=publication_time`.
   - Selects vacancies where "Rust" appears in the title.
   - Extracts contact details and a job link when possible.
   - Can poll built-in public proxy providers for Russian free proxies, probe them against the RSS endpoint, and retry the real request through the first working routes before falling back to direct access.
2. **Telegram module**
   - Uses the Bot API to send messages.
   - Stores the token and channel ID in the configuration.
3. **Scheduler**
   - Runs the update process every 20 minutes, typically via GitHub Actions.
   - Requests vacancies starting from the timestamp of the last successful run, with a small overlap window to avoid gaps near schedule boundaries.
   - Removes completed workflow runs older than three days using the `cleanup-old-runs` job.

## Data Flow
1. The scheduler initiates the task.
2. The hh_feed module requests vacancies since the last committed successful run and filters the relevant ones.
3. The Telegram module publishes a message with the list of vacancies.
4. The publication state is committed only when the run completes successfully.

All modules are implemented in Rust. The configuration is expected in a `.env` file.
