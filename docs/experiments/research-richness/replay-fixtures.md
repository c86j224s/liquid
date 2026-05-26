# Replay Fixtures

## Frozen Fixture Root

- Local sensitive fixture root: temp-only local archive outside the repository
- Replay pass report: `docs/experiments/research-richness/runs/artifact-finalization-replay-20260515T133816Z.md`
- Live gate report: temp-only local archive for `conflict-debt-live-gate-20260515T142032Z`; the older `targeted-live-rerun4-20260515T132800Z` report is historical pre-fix evidence only.

## Fixture Cases

- `1-02-current-policy-regulatory-*`
  - case id: `02-current-policy-regulatory`
  - scope: NIST AI RMF vs EU AI Act frontier-model obligations
- `2-03-comparative-product-technical-decision-*`
  - case id: `03-comparative-product-technical-decision`
  - scope: Apple MacBook Pro vs Framework Laptop local Rust and AI workflow decision

## Required Bundle Shape

Each frozen case bundle must contain exactly:

- `*-final-output.md`
- `*-controller-artifacts.json`
- `*-source-diagnostics.json`
- `*-resolved-system-prompt.md`
- `*-resolved-user-prompt.md`
- `*.json`

Missing files are fixture-setup failures, not research failures.

## Failure Contract

- Pre-finalization live bundles may be `quality_status=untrusted`.
- Replay consumes persisted artifacts, diagnostics, and final output only.
- Replay must not call live models, provider APIs, web search, or scrape fetches.
- Replay reports must not copy raw fixture bundles into the repository.
- Replay reports must fail closed on missing files, malformed artifact envelopes, secret markers, or raw-provider-payload leakage.

## Sensitivity Rules

- Treat resolved prompts, diagnostics, and per-case final outputs under `/tmp` as sensitive local debugging material.
- Do not commit raw `/tmp` bundles or provider payloads.
- Repository docs may reference fixture paths, replay outcomes, and bounded live-gate results, but not embed raw prompt captures or raw provider responses.

## Commands

Deterministic replay:

```bash
cargo run --bin research_bench -- \
  --mode replay \
  --label artifact-finalization-replay-20260515T133816Z \
  --replay-fixture-root /path/to/frozen-replay-bundle \
  --runs-dir docs/experiments/research-richness/runs
```

Bounded live gate after replay passes:

```bash
cargo run --bin research_bench -- \
  --mode live \
  --label targeted-live-rerun-post-finalization \
  --cases-dir docs/experiments/research-richness/live-gate-cases \
  --model-input cli:codex \
  --engine-name "Codex CLI" \
  --model-name codex
```
