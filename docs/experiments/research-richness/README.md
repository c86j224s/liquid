# Research Richness Benchmark

This experiment now gates live research quality with artifact-backed finalization and deterministic replay. The controller persists evidence artifacts and diagnostics first, the runtime rebuilds visible verification sections from those artifacts before final save, and frozen replay fixtures must pass before any further live pilot expands.

Live benchmark artifacts can contain resolved prompts, source diagnostics, URLs, and other model-facing research context. Treat the generated per-case artifact directories as sensitive debugging material rather than commit-ready documentation.

Use [workflow.md](./workflow.md) as the operational playbook when a future request asks to improve research quality along specific axes. [bench.toml](./bench.toml) records the reusable package contract and artifact hygiene policy for this benchmark.

The Narrative State improvement pass is part of the benchmark history, but its detailed phase artifacts are intentionally not included in this sanitized snapshot.
Only the derived benchmark summaries, CSV score tables, and reviewed sanitized `*-final-output.md` artifacts are committed here.

Reader Quality is now an additive hidden artifact layer inside the existing controller envelope. It can persist compact `argument_graph`, `narrative_plan`, `section_briefs`, and `reader_critique` planning artifacts for replay and benchmark analysis, but those fields stay non-evidentiary and are stripped from commit-safe final-output markdown together with the rest of `[RESEARCH_ARTIFACT_JSON]`.

The controller now has a bounded narrative/artifact enrichment stage between artifact persistence and finalization. The first enabled strategy is strict/high historical event-card enrichment: weak `narrative_state.event_cards` are selected, a compact JSON-only model call may propose missing phase fields plus nested `causal_spine` and `interpretive_layers`, and only fields grounded in existing Source Cards / Claim Log rows are merged. Unsupported or ungrounded proposals become `ResearchDebtItem`s; hidden narrative artifacts remain non-evidentiary. Non-historical modes are unchanged in this pass, but the app orchestration is intentionally shaped so future technology, policy, local recommendation, or comparative-decision enrichment strategies can plug into the same bounded stage.

For repeatable improvement passes, use [improvement-run.sh](./improvement-run.sh). It runs `research_bench` with the standard strict settings and, when `--preserve-dir` is provided, copies only commit-safe CSV and sanitized final-output artifacts while leaving aggregate markdown, raw diagnostics, prompts, and machine-readable artifact JSON in the raw run directory.

## Modularization Contract Freeze

Before workspace extraction, the benchmark surface is frozen at these boundaries:

- Binary entry point: `cargo run --bin research_bench -- ...`
- Long-form CLI flags: `--cases-dir`, `--runs-dir`, `--label`, `--mode`, `--replay-fixture-root`, `--data-dir`, `--model-input`, `--engine-name`, `--model-name`, `--research-intensity`, `--quality-depth`, `--max-iterations`, `--cli-launch-mode`, `--ai-task-timeout-secs`, `--include-raw-debug-artifacts`
- Mode contract: `fixture`, `live`, and `replay` remain the only benchmark modes
- Output contract: per-run `.md`, local-only `.json/.csv/.ndjson`, and reviewed sanitized `*-final-output.md` artifacts keep their current filenames and meanings
- Hygiene contract: fixture/replay can write under `docs/experiments/research-richness/runs/`; live mode defaults to an OS temp directory when `--runs-dir` or `LIQUID_BENCH_RUNS_DIR` is not provided
- Sanitized artifact contract: raw diagnostics, provider payloads, resolved prompts, controller-artifact JSON, local DBs, and generated raw run directories stay out of committed artifacts
- Dependency direction for modularization: `server -> service -> {research-core, runtime, acquisition, storage} -> contracts`, and `bench -> {research-core, runtime, acquisition, contracts}` without a `server` dependency

## Current Checkpoint

- Deterministic replay pass: `docs/experiments/research-richness/runs/artifact-finalization-replay-20260515T133816Z.md`
- Bounded live gate pass: `conflict-debt-live-gate-20260515T142032Z` completed `2/2` trusted, `0` critical, with raw outputs kept in temp-only storage outside the repository.
- `targeted-live-rerun4-20260515T132800Z` is historical pre-fix evidence only and should not be cited as current proof.
- Strict thresholds remained unchanged for replay and live verification.
- Replay and live fixture inputs remain local-sensitive under `/tmp`; only derived reports live in the repository.
- Repair search hints are transient leads only; hint URLs stay untrusted until normal adoption or independent fetch, and the final output should not echo repair-hint labels.
- Provider diversification and context/window tuning remain deferred follow-up work, and broader live pilots should stay cautious despite the bounded gate pass.
- Historical report structure was strengthened in the server-side `historical` research lens after the Aurelian source-reliability sample remained too thin. The follow-up live run `history-server-integration-aurelian-live-20260517T000000Z` improved final-output length and `genre_section_richness` (`1` to `4` versus the previous ad-hoc run), but still had one historical overlay critical flag for missing consequence signals. Treat this as an incremental product improvement, not a benchmark-clean pass.
- 2026-05-28 live API smoke tests for broad historical topics, including `러일전쟁`, still completed as `untrusted` even after the hidden planning-artifact and phase-dossier gates were added. The useful regression signal was that the model could produce Source Cards, Claim Log rows, and multiple visible event cards while still leaving grounded per-card `causal_spine` and `interpretive_layers` empty, so these runs are negative evidence for narrative-depth success. The raw task outputs, diagnostics, and controller artifacts remain local-only under the operator data directory; this snapshot records only the engineering lesson.

## Search Providers

Live source-pack discovery defaults to DuckDuckGo and requires no secrets.

Optional multi-provider discovery can be enabled with:

```bash
LIQUID_RESEARCH_SEARCH_PROVIDERS="naver,kakao,duckduckgo"
NAVER_CLIENT_ID=...
NAVER_CLIENT_SECRET=...
KAKAO_REST_API_KEY=...
```

The provider list is tried in order per query. This lets live mode prefer free API-key providers like Naver Search API and Kakao Daum Search API before falling back to DuckDuckGo.

Optional Brave Search API discovery remains available:

```bash
LIQUID_RESEARCH_SEARCH_PROVIDER=brave
BRAVE_SEARCH_API_KEY=...
```

If no configured provider list resolves to a usable provider, the runtime falls back to the existing single-provider selector and then to DuckDuckGo. API keys are sent only as request headers and are not written into source diagnostics.

Live discovery HTTP requests default to an 8-second timeout. For slow providers or regional networks, set `LIQUID_RESEARCH_SOURCE_HTTP_TIMEOUT_SECS` to a positive number of seconds; invalid values are ignored and values above 60 seconds are clamped to 60.

Source-pack status remains compatibility-stable and should be read like this in live mode:

- `blocked`: all live discovery queries were blocked and no seeded candidates were available
- `partial`: some queries were blocked or failed, so the report used only seeded or otherwise available candidates
- `empty`: no candidates were available, but the provider was not explicitly blocked
- `success`: discovery completed without blocked or error query reports

## What This Harness Records

Local benchmark runs may emit:

- case prompt and category
- requested engine/model metadata
- resolved prompt files captured from the real controller loop
- final output path or task failure note
- source diagnostics JSON artifact path
- controller artifacts JSON artifact path
- per-case structured rubric dimensions with assessor/source labels
- critical failure flags that stay visible per case even when run averages look healthy
- aggregate JSON, flat CSV, and NDJSON rows for graphing fixture or live runs

In this sanitized snapshot, the committed benchmark surface is narrower:

- `runs/*.md`: durable run summaries
- `runs/*.csv`: compact score tables selected for review
- `runs/*/*-final-output.md`: reviewed, sanitized reader-facing outputs only

## Structured Reporting Outputs

Each run can write the following local artifacts:

- `runs/<label>.md`: human-readable summary generated from the structured data
- `runs/<label>.json`: authoritative machine-readable run aggregate with per-case scorecards; replay cases also include additive `replay_before` state
- `runs/<label>.csv`: one row per case with overall score, status visibility, the 9 rubric dimensions, additive replay-before columns when applicable, and additive reader-quality metric columns appended at the end
- `runs/<label>.ndjson`: one row per case-dimension score for graphing or notebook analysis, plus additive replay-before state when applicable
- `runs/<label>/<case>.json`: per-case summary with links to raw artifacts plus the embedded structured scorecard

This repository does not commit the `.json`, `.ndjson`, or per-case summary `.json` artifacts. Those remain local review material unless a future sanitization pass says otherwise.

The benchmark scorer is the harness itself, not the model. Any model-emitted `0-5 Score` is retained only as advisory metadata and is never used as the authoritative aggregate score.

`measurement_kind` is mode-specific:

- fixture: `pipeline_evidence`
- replay: `artifact_backed_replay_validation`
- live: `live_non_deterministic_quality_measurement`

Fixture-mode structured scores are **pipeline evidence only**. They show whether the controller loop, artifact persistence, source diagnostics, and rubric-reporting pipeline are working end-to-end with visible failures. They do **not** prove live research quality.

Replay-mode structured scores are **deterministic validation evidence** for frozen live bundles. They prove the finalizer and strict validator can recover trusted visible outputs from persisted artifacts without fresh model or network calls.

## Cases

- `cases/01-product-decision.md`
- `cases/02-local-recommendation.md`
- `cases/03-historical-explanation.md`
- `cases/04-weak-source-rumor.md`
- `cases/05-conflicting-current-events.md`
- `cases/06-numeric-comparison.md`
- `cases/07-niche-troubleshooting.md`

Bounded live-gate cases:

- `live-gate-cases/02-current-policy-regulatory.md`
- `live-gate-cases/03-comparative-product-technical-decision.md`

## Usage

Deterministic fixture baseline through the reusable improvement harness:

```bash
docs/experiments/research-richness/improvement-run.sh --label baseline-dry-run
```

Live headless run with temp raw artifacts and optional durable preservation:

```bash
MODEL_INPUT="cli:codex" \
ENGINE_NAME="Codex CLI" \
MODEL_NAME="codex" \
docs/experiments/research-richness/improvement-run.sh \
  --mode live \
  --label baseline-live
```

`MODEL_INPUT` is required for live mode. `improvement-run.sh` keeps raw artifacts in a temp directory by default and fails fast before invoking Cargo if `MODEL_INPUT` is unset.

Reusable improvement pass with temp raw artifacts and optional durable preservation:

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

Deterministic replay against frozen live bundles:

```bash
cargo run --bin research_bench -- \
  --mode replay \
  --label artifact-finalization-replay-20260515T133816Z \
  --replay-fixture-root /path/to/frozen-replay-bundle \
  --runs-dir docs/experiments/research-richness/runs
```

One bounded two-case live rerun gate after replay passes:

```bash
cargo run --bin research_bench -- \
  --mode live \
  --label targeted-live-rerun-post-finalization \
  --cases-dir docs/experiments/research-richness/live-gate-cases \
  --model-input cli:codex \
  --engine-name "Codex CLI" \
  --model-name codex
```

When live mode is launched without an explicit `LIQUID_BENCH_RUNS_DIR` or `--runs-dir`, the Rust benchmark binary now defaults its output root to an OS temp directory outside the repository. Fixture mode continues to default to `docs/experiments/research-richness/runs/` for easy local inspection.

Optional isolated data dir base:

```bash
LIQUID_BENCH_DATA_DIR="./tmp/research-bench" \
docs/experiments/research-richness/improvement-run.sh --label baseline-dry-run
```

Optional explicit run artifact root:

```bash
LIQUID_BENCH_RUNS_DIR="./tmp/research-bench-runs" \
MODEL_INPUT="cli:codex" \
docs/experiments/research-richness/improvement-run.sh \
  --mode live \
  --label baseline-live
```

Fixture and replay runs can write their markdown summary to `docs/experiments/research-richness/runs/<label>.md` and sanitized final outputs under `docs/experiments/research-richness/runs/<label>/`. Live runs without an explicit run root print an out-of-repo temp path instead. Newly generated per-case directories should stay local until their sanitized final outputs are explicitly reviewed and added.

## Scoring Workflow

1. Run fixture mode first if you need pipeline smoke coverage across the broader benchmark set.
2. Run replay mode against frozen live bundles before any new live rerun.
3. Use the emitted local `.json`, `.csv`, and `.ndjson` aggregates to confirm `quality_status=passed`, zero critical flags, and visible Final Answer / Source Audit / Claim Log / Quality Gate sections.
4. Run at most one bounded two-case live gate after replay passes.
5. Keep strict thresholds unchanged; treat any pressure to lower them as a stop condition.
6. Keep live prompt and diagnostics artifacts in temp output unless an explicit redaction policy is in place.
