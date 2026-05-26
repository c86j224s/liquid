# Contributing

## Commit Messages

This repository uses Conventional Commits for release automation.

Use one of these prefixes for changes that should affect releases:

- `fix:` for patch releases.
- `feat:` for minor releases.
- `feat!:` or `fix!:` for breaking major releases.

Use non-release prefixes for maintenance work:

- `chore:`
- `docs:`
- `test:`
- `refactor:`
- `ci:`

Examples:

```text
feat: add queued scrape execution
fix: preserve scraped code block line breaks
ci: add release automation
```

## Module Boundaries

Keep new backend logic in the owning `src/*.rs` module and colocate Rust unit tests in that module's `#[cfg(test)] mod tests`. Keep frontend code as native ES modules under `static/`; do not add a bundler or framework without a separate architecture decision.
