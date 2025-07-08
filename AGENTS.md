# AGENT Instructions

- Responses may be in Russian or English as appropriate.
- All code comments and technical documentation must be written in English.
- Interpret user questions as tasks whenever possible and prefer providing a full merge request solution instead of a short code snippet.
- Install required Rust components with `rustup component add clippy rustfmt`.
- After making any changes, run `cargo fmt --all`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test`.
- Fix every issue reported by these commands before committing or submitting pull requests.
- A pull request is complete only when formatting, linting, tests, and `cargo machete` all succeed.
- Configure the remote `origin` as `https://github.com/qqrm/rust-hh-feed`.
- Read all Markdown (`*.md`) files from the `.docs` directory before starting work. Markdown in tests can be ignored.
- Avoid committed dead code; remove unused functions or feature-gate them.

## Additional User Notes
- The user often relies on voice input, so typos are possible; consider the context when interpreting requests.
- All source code, comments, and Markdown documentation must be written in English.
