# Repository Guidance

- This repository is a sanitized snapshot of Liquid. Keep source, docs, and safe configuration examples only.
- Do not commit `.env`, local databases, logs, `.fleet/`, `.gemini/`, `.claude/`, `.antigravitycli/`, or generated benchmark artifact directories.
- Research benchmark raw outputs stay outside the repository. Commit only durable summaries, CSV aggregates, and sanitized `*-final-output.md` artifacts when they are intentionally reviewable.
- Public license terms are not settled yet. Do not add Cargo `license` metadata or an OSI license file until the project owner decides the license.
