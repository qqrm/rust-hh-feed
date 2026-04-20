# Publication State Storage Architecture

This document describes how the bot stores information about vacancies that have already been sent to the Telegram channel. The data is saved in the `posted_jobs.json` file, which is persisted between runs as a pipeline artifact rather than in the repository.

## `posted_jobs.json` Format

The file is a JSON object with metadata about the last committed run and a dictionary of sent vacancies. Each key in `jobs` is a HeadHunter vacancy ID and the value is the publication date in the `YYYY-MM-DD` format.

```json
{
  "version": 2,
  "last_successful_run_at": "2026-04-03T07:56:51Z",
  "jobs": {
    "12345678": "2024-07-08",
    "87654321": "2024-07-09"
  }
}
```

## Typical Workflow

1. The CI workflow downloads `posted_jobs.json` from the previous successful run and places it in the `data` directory.
2. On startup the bot loads the file into memory, reads `last_successful_run_at`, and requests vacancies from the HeadHunter search RSS feed starting at that timestamp with a small overlap window.
3. If the file is missing or does not contain a committed success timestamp yet, the bot falls back to a wider bootstrap window so that delayed or unstable workflow schedules do not cause immediate data loss.
4. For each vacancy found:
   - if the ID already exists in the file, the vacancy is skipped;
   - otherwise the bot publishes it and adds a record to the JSON.
5. After posting, the bot removes entries older than `JOB_RETENTION_DAYS` (30 by default).
6. Only after the full run succeeds does the bot update `last_successful_run_at` and write the new state file.
7. The file is then uploaded as an artifact so that the next pipeline run can download it.

If a workflow run fails, its state file is not uploaded and its vacancies are not considered committed. The next successful run re-fetches vacancies since the previous successful run, which allows the bot to recover jobs discovered during failed runs.

## Why JSON

- Simplicity: `serde_json` is used for reading and writing.
- Fast lookup by ID with a dictionary in memory.
- The file remains small even with many records.
- The success timestamp allows the bot to recover after one or more failed workflow runs without trusting uncommitted state.

This approach prevents duplicate postings while keeping a history of sent vacancies.
