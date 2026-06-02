# Research Quality Improvement Playbook

Use this playbook when a request says to improve research quality along specific axes. The goal is to turn the request into an executable benchmark or live run without re-litigating the workflow each time.

The reusable package contract is recorded in `bench.toml`. Treat that file as a convention manifest for humans and future automation; `research_bench` remains the execution engine.

For strict/high historical runs, the bounded pre-finalization queue is now the controller surface for routed artifact stabilization, source/claim readiness repair, phase-state review, bounded narrative enrichment, final-answer rendering, and acceptance review. It records typed work items, mapped debt, dependencies, layered wave/model-call/work-item/attempt budgets, sanitized fingerprints, and conservative terminal routing (`accepted`, `partial_trusted`, `blocked_needs_user`, `budget_exhausted`, `no_progress`, `failed`) without storing prompts, provider payloads, raw model output, or diagnostics.

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
| "Make historical reports richer but still readable" | Use historical cases, preserve reader-facing prose, strengthen chronology, source layers, contested points, follow-up trails, inspect hidden `narrative_state`/`reader_quality` planning artifacts as non-evidentiary diagnostics for strict/high runs, and keep visible phase density for campaign-history subjects such as Hannibal / Second Punic War. |
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
| Better historical writing | Historical overlay gates plus Genre/section richness, with hidden historical planning-artifact depth checked separately from evidence |
| Better technical guidance | Direct answer usefulness, practical next steps, Genre/section richness |
| Better decision support | User intent fit, practical next steps, information resolution |
| Less rumor leakage | Official/rumor/opinion separation |
| Better structure | Structure and readability |
| Better reader flow or narrative coherence | Structure and readability plus the additive reader-quality diagnostics (`argument_graph`, `narrative_plan`, `section_briefs`, `reader_critique`) |

Shared dimensions stay unchanged across categories. The only benchmark-only extra is the category-aware `Genre/section richness` dimension. Historical categories also add the historical overlay gates and score caps described in `rubric.md`. For strict/high historical cases, keep the visible richness markers and the hidden planning artifacts aligned: empty `narrative_state` or `reader_quality` should trigger review warnings and follow-up work, but those hidden artifacts remain non-evidentiary diagnostics rather than standalone pass/fail evidence.

For narrow campaign-history subjects where regression risk is known, prefer a structural floor instead of exemplar prose. Hannibal / Second Punic War is the current example: preserve event-card-driven visible phase subsections, date anchors, and actor/front anchors so a later draft cannot collapse into a short summary while still looking superficially complete.

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
- Do not commit raw Reader Quality prompt scaffolds separately. They live only inside the existing hidden controller-artifact envelope and stay out of commit-safe markdown.
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


### Historical phase dossier gate

Strict/high broad-history runs should not treat `event_cards` as mere chronology rows. Important cards should include grounded `causal_spine` steps and grounded `interpretive_layers`; the visible report should expand those dossiers into phase prose while the verification appendix remains the evidence ledger. Flat claim-linked cards are useful as fallback diagnostics but should not be reported as narrative-depth success.

A bounded resumable controller work queue now runs after artifact parsing/persistence and before finalization. In strict/high historical mode the queue executes routed bounded handlers in small waves: artifact stabilization, source-card repair, claim-log repair, phase plan build/review, phase-claim readiness, event-card enrichment, causal-continuity review, final-answer render, and research acceptance review. It persists only sanitized queue checkpoints/budgets/terminal status inside `research_controller_artifacts_json`, maps relevant open debt back onto typed work items, and stops on completion, blocked/no-progress conditions, or budget exhaustion. The default strict/high budget is conservative: three waves, up to twelve routed model-call slots, up to three queued work items per wave capped by two routed attempts per wave, two attempts per individual work item, and two no-progress waves before terminal routing. The strict/high historical `phase_state` step still runs first: it treats model-emitted event cards as candidates, rejects placeholders such as generic “phase/국면/근거 연결 국면” labels, derives phase candidates from event cards, section outline, timeline, open gaps, and known broad-process topic patterns, and repairs only grounded structural card identity. Phase cards are not accepted as ready until they have phase-specific supported Claim Log rows; broad whole-topic claims can provide context but cannot alone ground a concrete phase card. The first model-backed strategy remains historical event-card enrichment. That strategy selects only weak ready cards, calls the existing `ModelRuntime` with web search disabled and a compact JSON-only prompt, keeps that internal prompt transient, merges only fields grounded in existing public Source Cards / supported Claim Log rows, and turns invalid JSON, invented IDs, private/local source refs, or ungrounded detail into specific research debt. Non-historical modes should remain behavior-preserving until a future strategy is explicitly added.

Artifact stabilization is a support step, not the quality strategy: oversized but syntactically valid machine artifact JSON is salvaged into compact controller artifacts when safe, with warnings/debt recorded, so Source Cards and Claim Log rows are not lost before `phase_state` / `narrative_enrichment`. Malformed JSON still fails safely; raw oversized JSON, provider payloads, source diagnostics, and resolved prompts remain outside committed outputs.

Interpretation is allowed and expected. The gate should reject unsupported facts and illogical inference, not all inference: use `epistemic_status`, `reasoning`, and `limits` to keep fact anchors, grounded interpretation, and one-step inference distinct. A useful historical report is not just fact checking; it explains what the evidence means and where the reasoning would break.
