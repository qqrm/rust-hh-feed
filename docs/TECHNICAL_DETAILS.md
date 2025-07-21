# Publication State Storage Architecture

This document describes how the bot stores information about vacancies that have already been sent to the Telegram channel. The data is saved in the `posted_jobs.json` file, which is persisted between runs as a pipeline artifact rather than in the repository.

## `posted_jobs.json` Format

The file is a dictionary where each key is a HeadHunter vacancy ID and the value is the publication date in the `YYYY-MM-DD` format.

```json
{
  "12345678": "2024-07-08",
  "87654321": "2024-07-09"
}
```

## Typical Workflow

1. The CI workflow downloads `posted_jobs.json` from the previous successful run and places it in the `data` directory.
2. On startup the bot loads the file into memory (for example, as `HashMap<String, String>`).
3. For each vacancy found:
   - if the ID already exists in the file, the vacancy is skipped;
   - otherwise the bot publishes it and adds a record to the JSON.
4. After posting, the bot removes entries older than `JOB_RETENTION_DAYS` (30 by default).
5. The file is then updated and uploaded as an artifact so that the next pipeline run can download it.

## Why JSON

- Simplicity: `serde_json` is used for reading and writing.
- Fast lookup by ID with a dictionary in memory.
- The file remains small even with many records.

This approach prevents duplicate postings while keeping a history of sent vacancies.
