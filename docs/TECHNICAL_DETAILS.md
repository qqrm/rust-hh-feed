# Publication State Storage Architecture

This document describes how the bot stores information about vacancies that have already been sent to the Telegram channel. The data is kept in the `data/posted_jobs.json` file.

## `posted_jobs.json` Format

The file is a dictionary where each key is a HeadHunter vacancy ID and the value is the publication date in the `YYYY-MM-DD` format.

```json
{
  "12345678": "2024-07-08",
  "87654321": "2024-07-09"
}
```

## Typical Workflow

1. On startup the bot loads `posted_jobs.json` into memory (for example, as `HashMap<String, String>`).
2. For each vacancy found:
   - if the ID already exists in the file, the vacancy is skipped;
   - otherwise the bot publishes it and adds a record to the JSON.
3. After posting, the file is updated and committed back to the repository automatically.

## Why JSON

- Simplicity: `serde_json` is used for reading and writing.
- Fast lookup by ID with a dictionary in memory.
- The file remains small even with many records.

This approach prevents duplicate postings while keeping a history of sent vacancies.
