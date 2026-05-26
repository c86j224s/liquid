# Research Quality Improvement Playbook

Use this playbook when a request says to improve research quality along specific axes. The goal is to turn the request into an executable benchmark or live run without re-litigating the workflow each time.

The reusable package contract is recorded in `bench.toml`. Treat that file as a convention manifest for humans and future automation; `research_bench` remains the execution engine.

## 1. Request Intake Template

Capture these fields before doing any work:

- Request goal: what should improve, and for which audience.
- Axes to improve: list the concrete quality axes or dimensions.
- Target mode: `fixture`, `replay`, or `live`.
- Target cases: benchmark set, live-gate set, or both.
- Constraints: provider limits, runtime limits, output location limits, secret handling rules.
- Required evidence: score deltas, passing gates, artifact shapes, or reading judgment.
- Output requirement: markdown report, CSV/JSON aggregate, or both.

Fast path for future requests:

| Future request shape | Immediate interpretation |
| --- | --- |
| "Improve research quality for source richness" | Add or adjust source-pack/search cases, check source diversity, and verify that source-pack status is honest. |
| "Make historical reports richer but still readable" | Use historical cases, preserve reader-facing prose, and strengthen chronology, source layers, contested points, and follow-up trails. |
| "Make technical research more useful" | Use technical implementation cases, score design judgment, tradeoffs, verification strategy, and practical next steps. |
| "The score looks too generous" | Run replay or fixture scoring first, inspect source-pack and rubric dimensions, then fix scoring before any new live run. |
| "The answer reads well but sources feel weak" | Separate final-answer quality from source-pack quality and validate adopted sources before trusting the score. |

If the request is vague, default to:

- `fixture` first for pipeline smoke coverage
- `replay` next for deterministic validation
- `live` only after the replay path is stable

## 2. Quality-Axis Mapping

Map user language to the shared rubric before writing prompts or scoring results.

| User asks for | Use these rubric axes |
| --- | --- |
| Better evidence or sourcing | Evidence/claim traceability, source quality and diversity |
| Better uncertainty handling | Handling of uncertainty and conflicts |
| Better historical writing | Historical overlay gates plus Genre/section richness |
| Better technical guidance | Direct answer usefulness, practical next steps, Genre/section richness |
| Better decision support | User intent fit, practical next steps, information resolution |
| Less rumor leakage | Official/rumor/opinion separation |
| Better structure | Structure and readability |

Shared dimensions stay unchanged across categories. The only benchmark-only extra is the category-aware `Genre/section richness` dimension. Historical categories also add the historical overlay gates and score caps described in `rubric.md`.

## 3. When To Use Live vs Replay

Use `replay` when:

- You need to validate artifact finalization or strict gate behavior.
- You have a frozen live bundle and want deterministic validation.
- You want to check whether a known run still reproduces the same trusted visible output.

Use `live` when:

- You need actual source discovery, provider behavior, or non-deterministic model behavior.
- You need to judge whether the pipeline works against current sources.
- Replay already passed and the next step is a bounded live gate.

Use `fixture` when:

- You need a cheap smoke test of the controller loop and reporting pipeline.
- You want to confirm output shape before spending live budget.

## 4. Gate Ladder

Treat the gates as an ordered checklist, not as interchangeable labels.

### Artifact review gate

Purpose: verify artifact integrity and replay consistency.

- Confirm the replay bundle exists and is complete.
- Confirm structured artifacts, final output, and visible verification sections are all present.
- Fail if replay regresses, artifacts are malformed, or raw payloads leak.

### QA review gate

Purpose: verify source relevance and source-pack quality.

- Confirm adopted sources are topical and authority-aware.
- Fail if off-topic adopted sources inflate source-pack success.
- Fail if source-pack status hides missing relevance or weak authority.

### Architecture review gate

Purpose: preserve the authority-gated path and check final reader-facing quality.

- Keep the C++ authority-gate path intact if it is the current successful path.
- Fail if a live run looks good in prose but source-pack relevance scoring is still untrusted.
- Do not expand live pilots until the source-pack bug has regression coverage and is fixed.

## 5. Artifact Commit Policy

Commit only durable, reviewable markdown and score summaries:

- Commit: `README.md`, `final-report.md`, evaluation docs, run summaries, compact CSV aggregates, and reviewed sanitized `*-final-output.md` artifacts when they are meant to be durable records.
- Do not commit: raw diagnostics, resolved prompts, controller artifact dumps, provider payloads, or temp live outputs.
- If a run produces per-case raw artifacts, keep them in temp or local archive storage and reference them from the summary instead of copying them into the repo. Only sanitized reader-facing final outputs should be promoted into `runs/<label>/`.

If a request explicitly asks for a durable replay or comparison artifact, commit the derived summary only, not the raw bundle.

## 6. Raw Diagnostics Archive Policy

- Keep raw diagnostics in temp output or a local archive outside the repo.
- Treat source diagnostics, controller artifacts, and resolved prompts as sensitive.
- If a diagnostic is needed for review, summarize it in markdown and link the derived report rather than embedding the raw JSON.
- Only archive raw outputs in-repo if the request explicitly requires it and the content has been sanitized.

## 7. Minimal Command Examples

Fixture smoke:

```bash
docs/experiments/research-richness/improvement-run.sh --label smoke-fixture
```

Deterministic replay:

```bash
cargo run --bin research_bench -- \
  --mode replay \
  --label artifact-finalization-replay \
  --replay-fixture-root /path/to/frozen-replay-bundle \
  --runs-dir docs/experiments/research-richness/runs
```

Bounded live gate:

```bash
MODEL_INPUT="cli:codex" \
ENGINE_NAME="Codex CLI" \
MODEL_NAME="codex" \
cargo run --bin research_bench -- \
  --mode live \
  --label bounded-live-gate \
  --cases-dir docs/experiments/research-richness/live-gate-cases
```

If live output should stay outside the repository, set `LIQUID_BENCH_RUNS_DIR` to a temp path.

## 8. Reusable Improvement Run

Use `improvement-run.sh` when the user asks for another quality improvement pass and the goal is to avoid rebuilding this workflow manually.

Default fixture smoke with temp raw artifacts:

```bash
docs/experiments/research-richness/improvement-run.sh \
  --label source-richness-fixture
```

Bounded live pass with commit-safe preservation:

```bash
MODEL_INPUT="cli:codex" \
ENGINE_NAME="Codex CLI" \
MODEL_NAME="codex" \
docs/experiments/research-richness/improvement-run.sh \
  --mode live \
  --label bounded-live-gate \
  --cases-dir docs/experiments/research-richness/live-gate-cases \
  --preserve-dir ./tmp/research-richness-preserved/$(date -u +%Y-%m-%d)
```

The helper keeps raw outputs in a temp directory by default. With `--preserve-dir`, it copies only:

- `runs/<label>.csv`
- `runs/<label>/*-final-output.md` with `[RESEARCH_ARTIFACT_JSON]` blocks stripped
- `<label>-preservation-note.md`

It intentionally does not copy aggregate markdown, JSON, NDJSON, source diagnostics, controller artifacts, resolved prompts, provider payloads, or machine-readable artifact JSON embedded in final-output markdown.

## 9. End-Of-Run Checklist

Before finishing, confirm:

1. The request was mapped to the right mode and cases.
2. The relevant gate path passed or the blocker is explicit.
3. The final report states what improved, what stayed weak, and what is still blocked.
4. The committed artifacts are only the durable summaries, not raw diagnostics.
5. The user-facing result report names the key wins, known bugs, and next engineering priorities.
6. The repo status is clean enough for the next maintainer to resume without guessing.

## 10. Default Execution Pattern

When a new improvement request arrives, use this order unless the request is clearly trivial:

1. Anchor the requested quality axis in one sentence.
2. Recon the existing benchmark cases, recent run artifacts, and relevant scoring/source code.
3. Decide whether the issue is prompt/case design, source collection, scoring, finalization, or reader-facing prose.
4. Use replay or fixture tests to lock the failure when possible.
5. Use a bounded live run only when provider behavior or current-source discovery is part of the question.
6. Request an architecture review when deciding whether a score or architectural interpretation is trustworthy.
7. Run a QA review after implementation or scoring changes.
8. Update the evaluation note, workflow doc, or final report after those reviews land.
9. Commit code fixes separately from durable experiment artifacts.

## 11. Default Output Shape

When closing a request, aim to leave:

- one benchmark or evaluation markdown report
- one concise result summary for the user
- only the durable artifacts that need to be preserved

This keeps the workflow repeatable: intake, map, choose mode, run gates, commit summaries, report results.
