# Research Richness Run: fixture-smoke-after-fix-20260513-125845

- Timestamp: 2026-05-13T03:58:47.448481+00:00
- Mode: fixture (deterministic)
- Engine: Fixture Controller
- Model: fixture-research-bench

## Case Inventory
- product-decision: Current Product Decision With Specs And Tradeoffs (01-product-decision.md)
- local-recommendation: Local Recommendation With Constraints (02-local-recommendation.md)
- historical-explanation: Historical Chronology And Causality (03-historical-explanation.md)
- weak-source-rumor: Weak-Source Rumor And Unofficial Claim (04-weak-source-rumor.md)
- conflicting-current-events: Conflicting Current-Events Evidence (05-conflicting-current-events.md)
- numeric-comparison: Numerical Comparison With Dates Prices Or Versions (06-numeric-comparison.md)
- niche-troubleshooting: Sparse Technical Troubleshooting (07-niche-troubleshooting.md)

## Current Product Decision With Specs And Tradeoffs

- Category: product-decision
- Case file: 01-product-decision.md
- Prompt: Compare two current developer laptops for a local Rust and AI workflow, including thermals, memory ceilings, battery tradeoffs, and whether one is meaningfully safer for heavy local model work.
- Task status: completed
- Quality status: passed
- Quality last failure: none
- Final output: docs/experiments/research-richness/runs/fixture-smoke-after-fix-20260513-125845/1-01-product-decision-final-output.md
- Source diagnostics JSON: not included in sanitized snapshot
- Controller artifacts JSON: not included in sanitized snapshot
- Resolved system prompt: not included in sanitized snapshot
- Resolved user prompt: not included in sanitized snapshot

### Must-Pass Evidence Checks
- cites current specs with dates or release windows
- separates official specs from reviewer claims
- explains practical tradeoffs, not only score tables

### Expected Failure Modes
- stale specs presented as current
- pricing or availability fabricated
- recommendation ignores memory ceiling or thermal limits

### Artifact And Diagnostic Availability
- source diagnostics envelope: present
- controller artifacts envelope: present
- structured context packing diagnostics: present


## Local Recommendation With Constraints

- Category: local-recommendation
- Case file: 02-local-recommendation.md
- Prompt: Recommend a quiet Seoul coffee shop for a two-hour weekday work session near Line 2 with power outlets, light food, and no long detour from transit.
- Task status: completed
- Quality status: passed
- Quality last failure: none
- Final output: docs/experiments/research-richness/runs/fixture-smoke-after-fix-20260513-125845/2-02-local-recommendation-final-output.md
- Source diagnostics JSON: not included in sanitized snapshot
- Controller artifacts JSON: not included in sanitized snapshot
- Resolved system prompt: not included in sanitized snapshot
- Resolved user prompt: not included in sanitized snapshot

### Must-Pass Evidence Checks
- keeps neighborhood and transit constraints explicit
- distinguishes official venue info from map reviews or rumors
- notes uncertainty when outlet or seating data is weak

### Expected Failure Modes
- ignores location constraint
- treats review anecdotes as guaranteed facts
- recommends places without addressing quietness or outlets

### Artifact And Diagnostic Availability
- source diagnostics envelope: present
- controller artifacts envelope: present
- structured context packing diagnostics: present


## Historical Chronology And Causality

- Category: historical-explanation
- Case file: 03-historical-explanation.md
- Prompt: Explain why the Gothic War under Justinian expanded, why it lasted so long, and what consequences followed in Italy, with chronology and contested points separated clearly.
- Task status: completed
- Quality status: passed
- Quality last failure: none
- Final output: docs/experiments/research-richness/runs/fixture-smoke-after-fix-20260513-125845/3-03-historical-explanation-final-output.md
- Source diagnostics JSON: not included in sanitized snapshot
- Controller artifacts JSON: not included in sanitized snapshot
- Resolved system prompt: not included in sanitized snapshot
- Resolved user prompt: not included in sanitized snapshot

### Must-Pass Evidence Checks
- includes chronology, actors, causes, and consequences
- separates direct evidence from interpretation
- marks contested casualty or damage estimates as uncertain

### Expected Failure Modes
- timeline collapses into a simple summary
- causality is asserted without support
- contested claims are stated as settled fact

### Artifact And Diagnostic Availability
- source diagnostics envelope: present
- controller artifacts envelope: present
- structured context packing diagnostics: present


## Weak-Source Rumor And Unofficial Claim

- Category: weak-source-rumor
- Case file: 04-weak-source-rumor.md
- Prompt: Assess whether a rumored unreleased device feature is likely, what evidence exists, and what should still be treated as rumor versus official information.
- Task status: completed
- Quality status: passed
- Quality last failure: none
- Final output: docs/experiments/research-richness/runs/fixture-smoke-after-fix-20260513-125845/4-04-weak-source-rumor-final-output.md
- Source diagnostics JSON: not included in sanitized snapshot
- Controller artifacts JSON: not included in sanitized snapshot
- Resolved system prompt: not included in sanitized snapshot
- Resolved user prompt: not included in sanitized snapshot

### Must-Pass Evidence Checks
- labels rumor, leak, and official confirmation separately
- avoids overconfident recommendation from weak sourcing
- states what evidence would be needed to upgrade confidence

### Expected Failure Modes
- rumor treated as confirmed
- weak forum post outweighs official material
- no uncertainty handling

### Artifact And Diagnostic Availability
- source diagnostics envelope: present
- controller artifacts envelope: present
- structured context packing diagnostics: present


## Conflicting Current-Events Evidence

- Category: conflicting-current-events
- Case file: 05-conflicting-current-events.md
- Prompt: Summarize a fast-moving current event where early reports conflict, identify what is verified, and list what remains open.
- Task status: completed
- Quality status: passed
- Quality last failure: none
- Final output: docs/experiments/research-richness/runs/fixture-smoke-after-fix-20260513-125845/5-05-conflicting-current-events-final-output.md
- Source diagnostics JSON: not included in sanitized snapshot
- Controller artifacts JSON: not included in sanitized snapshot
- Resolved system prompt: not included in sanitized snapshot
- Resolved user prompt: not included in sanitized snapshot

### Must-Pass Evidence Checks
- compares conflicts explicitly instead of averaging them away
- uses dates and source classes to explain confidence
- leaves unresolved claims open when evidence is incomplete

### Expected Failure Modes
- early inaccurate report repeated as fact
- chronology missing or reversed
- conflict resolution is hidden from the reader

### Artifact And Diagnostic Availability
- source diagnostics envelope: present
- controller artifacts envelope: present
- structured context packing diagnostics: present


## Numerical Comparison With Dates Prices Or Versions

- Category: numeric-comparison
- Case file: 06-numeric-comparison.md
- Prompt: Compare two software hosting plans using current published prices, rate limits, and feature tiers, including the exact date context for each figure.
- Task status: completed
- Quality status: passed
- Quality last failure: none
- Final output: docs/experiments/research-richness/runs/fixture-smoke-after-fix-20260513-125845/6-06-numeric-comparison-final-output.md
- Source diagnostics JSON: not included in sanitized snapshot
- Controller artifacts JSON: not included in sanitized snapshot
- Resolved system prompt: not included in sanitized snapshot
- Resolved user prompt: not included in sanitized snapshot

### Must-Pass Evidence Checks
- numbers have date context
- plan differences are attributed to concrete sources
- unsupported arithmetic or stale pricing is avoided

### Expected Failure Modes
- prices mixed across dates or regions
- feature tiers misread
- unsupported totals or savings claims

### Artifact And Diagnostic Availability
- source diagnostics envelope: present
- controller artifacts envelope: present
- structured context packing diagnostics: present


## Sparse Technical Troubleshooting

- Category: niche-troubleshooting
- Case file: 07-niche-troubleshooting.md
- Prompt: Diagnose a niche local-model tool-calling failure in an open-source repo where documentation is sparse, and recommend the safest next debugging steps.
- Task status: completed
- Quality status: passed
- Quality last failure: none
- Final output: docs/experiments/research-richness/runs/fixture-smoke-after-fix-20260513-125845/7-07-niche-troubleshooting-final-output.md
- Source diagnostics JSON: not included in sanitized snapshot
- Controller artifacts JSON: not included in sanitized snapshot
- Resolved system prompt: not included in sanitized snapshot
- Resolved user prompt: not included in sanitized snapshot

### Must-Pass Evidence Checks
- distinguishes repo facts, issue-thread anecdotes, and inference
- proposes next checks tied to evidence gaps
- avoids pretending a fix is verified when only workarounds exist

### Expected Failure Modes
- unsupported root-cause certainty
- generic troubleshooting with no repo-specific evidence
- issue comments treated as canonical documentation

### Artifact And Diagnostic Availability
- source diagnostics envelope: present
- controller artifacts envelope: present
- structured context packing diagnostics: present
