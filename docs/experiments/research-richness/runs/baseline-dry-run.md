# Research Richness Run: baseline-dry-run

- Timestamp: 2026-05-13T00:37:35Z
- Mode: dry-run
- Engine: blocked
- Model: blocked
- Resolved system prompt: not included in sanitized snapshot
- Resolved user prompt: not included in sanitized snapshot

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
- Final output: blocked: runtime unavailable
- Source diagnostics JSON: not included in sanitized snapshot
- Controller artifacts JSON: not included in sanitized snapshot

### Must-Pass Evidence Checks
- cites current specs with dates or release windows
- separates official specs from reviewer claims
- explains practical tradeoffs, not only score tables

### Expected Failure Modes
- stale specs presented as current
- pricing or availability fabricated
- recommendation ignores memory ceiling or thermal limits

### Rubric Dimensions
- 1. User intent fit: blocked: runtime unavailable
- 2. Direct answer usefulness: blocked: runtime unavailable
- 3. Evidence/claim traceability: blocked: runtime unavailable
- 4. Source quality and diversity: blocked: runtime unavailable
- 5. Official/rumor/opinion separation: blocked: runtime unavailable
- 6. Handling of uncertainty and conflicts: blocked: runtime unavailable
- 7. Information resolution compared with available sources: blocked: runtime unavailable
- 8. Practical next steps or decision support: blocked: runtime unavailable
- 9. Structure and readability: blocked: runtime unavailable

### Critical Failure Flags
- Unsupported factual claim that affects the conclusion
- Fabricated source, URL, date, price, or named entity
- Fails to answer the user's actual question
- Treats rumor/opinion as official fact
- Ignores a major constraint from the prompt
- Produces final answer without meaningful evidence or uncertainty handling

### Artifact And Diagnostic Availability
- source diagnostics envelope: blocked
- controller artifacts envelope: blocked
- structured context packing diagnostics: blocked

## Local Recommendation With Constraints

- Category: local-recommendation
- Case file: 02-local-recommendation.md
- Prompt: Recommend a quiet Seoul coffee shop for a two-hour weekday work session near Line 2 with power outlets, light food, and no long detour from transit.
- Final output: blocked: runtime unavailable
- Source diagnostics JSON: not included in sanitized snapshot
- Controller artifacts JSON: not included in sanitized snapshot

### Must-Pass Evidence Checks
- keeps neighborhood and transit constraints explicit
- distinguishes official venue info from map reviews or rumors
- notes uncertainty when outlet or seating data is weak

### Expected Failure Modes
- ignores location constraint
- treats review anecdotes as guaranteed facts
- recommends places without addressing quietness or outlets

### Rubric Dimensions
- 1. User intent fit: blocked: runtime unavailable
- 2. Direct answer usefulness: blocked: runtime unavailable
- 3. Evidence/claim traceability: blocked: runtime unavailable
- 4. Source quality and diversity: blocked: runtime unavailable
- 5. Official/rumor/opinion separation: blocked: runtime unavailable
- 6. Handling of uncertainty and conflicts: blocked: runtime unavailable
- 7. Information resolution compared with available sources: blocked: runtime unavailable
- 8. Practical next steps or decision support: blocked: runtime unavailable
- 9. Structure and readability: blocked: runtime unavailable

### Critical Failure Flags
- Unsupported factual claim that affects the conclusion
- Fabricated source, URL, date, price, or named entity
- Fails to answer the user's actual question
- Treats rumor/opinion as official fact
- Ignores a major constraint from the prompt
- Produces final answer without meaningful evidence or uncertainty handling

### Artifact And Diagnostic Availability
- source diagnostics envelope: blocked
- controller artifacts envelope: blocked
- structured context packing diagnostics: blocked

## Historical Chronology And Causality

- Category: historical-explanation
- Case file: 03-historical-explanation.md
- Prompt: Explain why the Gothic War under Justinian expanded, why it lasted so long, and what consequences followed in Italy, with chronology and contested points separated clearly.
- Final output: blocked: runtime unavailable
- Source diagnostics JSON: not included in sanitized snapshot
- Controller artifacts JSON: not included in sanitized snapshot

### Must-Pass Evidence Checks
- includes chronology, actors, causes, and consequences
- separates direct evidence from interpretation
- marks contested casualty or damage estimates as uncertain

### Expected Failure Modes
- timeline collapses into a simple summary
- causality is asserted without support
- contested claims are stated as settled fact

### Rubric Dimensions
- 1. User intent fit: blocked: runtime unavailable
- 2. Direct answer usefulness: blocked: runtime unavailable
- 3. Evidence/claim traceability: blocked: runtime unavailable
- 4. Source quality and diversity: blocked: runtime unavailable
- 5. Official/rumor/opinion separation: blocked: runtime unavailable
- 6. Handling of uncertainty and conflicts: blocked: runtime unavailable
- 7. Information resolution compared with available sources: blocked: runtime unavailable
- 8. Practical next steps or decision support: blocked: runtime unavailable
- 9. Structure and readability: blocked: runtime unavailable

### Critical Failure Flags
- Unsupported factual claim that affects the conclusion
- Fabricated source, URL, date, price, or named entity
- Fails to answer the user's actual question
- Treats rumor/opinion as official fact
- Ignores a major constraint from the prompt
- Produces final answer without meaningful evidence or uncertainty handling

### Artifact And Diagnostic Availability
- source diagnostics envelope: blocked
- controller artifacts envelope: blocked
- structured context packing diagnostics: blocked

## Weak-Source Rumor And Unofficial Claim

- Category: weak-source-rumor
- Case file: 04-weak-source-rumor.md
- Prompt: Assess whether a rumored unreleased device feature is likely, what evidence exists, and what should still be treated as rumor versus official information.
- Final output: blocked: runtime unavailable
- Source diagnostics JSON: not included in sanitized snapshot
- Controller artifacts JSON: not included in sanitized snapshot

### Must-Pass Evidence Checks
- labels rumor, leak, and official confirmation separately
- avoids overconfident recommendation from weak sourcing
- states what evidence would be needed to upgrade confidence

### Expected Failure Modes
- rumor treated as confirmed
- weak forum post outweighs official material
- no uncertainty handling

### Rubric Dimensions
- 1. User intent fit: blocked: runtime unavailable
- 2. Direct answer usefulness: blocked: runtime unavailable
- 3. Evidence/claim traceability: blocked: runtime unavailable
- 4. Source quality and diversity: blocked: runtime unavailable
- 5. Official/rumor/opinion separation: blocked: runtime unavailable
- 6. Handling of uncertainty and conflicts: blocked: runtime unavailable
- 7. Information resolution compared with available sources: blocked: runtime unavailable
- 8. Practical next steps or decision support: blocked: runtime unavailable
- 9. Structure and readability: blocked: runtime unavailable

### Critical Failure Flags
- Unsupported factual claim that affects the conclusion
- Fabricated source, URL, date, price, or named entity
- Fails to answer the user's actual question
- Treats rumor/opinion as official fact
- Ignores a major constraint from the prompt
- Produces final answer without meaningful evidence or uncertainty handling

### Artifact And Diagnostic Availability
- source diagnostics envelope: blocked
- controller artifacts envelope: blocked
- structured context packing diagnostics: blocked

## Conflicting Current-Events Evidence

- Category: conflicting-current-events
- Case file: 05-conflicting-current-events.md
- Prompt: Summarize a fast-moving current event where early reports conflict, identify what is verified, and list what remains open.
- Final output: blocked: runtime unavailable
- Source diagnostics JSON: not included in sanitized snapshot
- Controller artifacts JSON: not included in sanitized snapshot

### Must-Pass Evidence Checks
- compares conflicts explicitly instead of averaging them away
- uses dates and source classes to explain confidence
- leaves unresolved claims open when evidence is incomplete

### Expected Failure Modes
- early inaccurate report repeated as fact
- chronology missing or reversed
- conflict resolution is hidden from the reader

### Rubric Dimensions
- 1. User intent fit: blocked: runtime unavailable
- 2. Direct answer usefulness: blocked: runtime unavailable
- 3. Evidence/claim traceability: blocked: runtime unavailable
- 4. Source quality and diversity: blocked: runtime unavailable
- 5. Official/rumor/opinion separation: blocked: runtime unavailable
- 6. Handling of uncertainty and conflicts: blocked: runtime unavailable
- 7. Information resolution compared with available sources: blocked: runtime unavailable
- 8. Practical next steps or decision support: blocked: runtime unavailable
- 9. Structure and readability: blocked: runtime unavailable

### Critical Failure Flags
- Unsupported factual claim that affects the conclusion
- Fabricated source, URL, date, price, or named entity
- Fails to answer the user's actual question
- Treats rumor/opinion as official fact
- Ignores a major constraint from the prompt
- Produces final answer without meaningful evidence or uncertainty handling

### Artifact And Diagnostic Availability
- source diagnostics envelope: blocked
- controller artifacts envelope: blocked
- structured context packing diagnostics: blocked

## Numerical Comparison With Dates Prices Or Versions

- Category: numeric-comparison
- Case file: 06-numeric-comparison.md
- Prompt: Compare two software hosting plans using current published prices, rate limits, and feature tiers, including the exact date context for each figure.
- Final output: blocked: runtime unavailable
- Source diagnostics JSON: not included in sanitized snapshot
- Controller artifacts JSON: not included in sanitized snapshot

### Must-Pass Evidence Checks
- numbers have date context
- plan differences are attributed to concrete sources
- unsupported arithmetic or stale pricing is avoided

### Expected Failure Modes
- prices mixed across dates or regions
- feature tiers misread
- unsupported totals or savings claims

### Rubric Dimensions
- 1. User intent fit: blocked: runtime unavailable
- 2. Direct answer usefulness: blocked: runtime unavailable
- 3. Evidence/claim traceability: blocked: runtime unavailable
- 4. Source quality and diversity: blocked: runtime unavailable
- 5. Official/rumor/opinion separation: blocked: runtime unavailable
- 6. Handling of uncertainty and conflicts: blocked: runtime unavailable
- 7. Information resolution compared with available sources: blocked: runtime unavailable
- 8. Practical next steps or decision support: blocked: runtime unavailable
- 9. Structure and readability: blocked: runtime unavailable

### Critical Failure Flags
- Unsupported factual claim that affects the conclusion
- Fabricated source, URL, date, price, or named entity
- Fails to answer the user's actual question
- Treats rumor/opinion as official fact
- Ignores a major constraint from the prompt
- Produces final answer without meaningful evidence or uncertainty handling

### Artifact And Diagnostic Availability
- source diagnostics envelope: blocked
- controller artifacts envelope: blocked
- structured context packing diagnostics: blocked

## Sparse Technical Troubleshooting

- Category: niche-troubleshooting
- Case file: 07-niche-troubleshooting.md
- Prompt: Diagnose a niche local-model tool-calling failure in an open-source repo where documentation is sparse, and recommend the safest next debugging steps.
- Final output: blocked: runtime unavailable
- Source diagnostics JSON: not included in sanitized snapshot
- Controller artifacts JSON: not included in sanitized snapshot

### Must-Pass Evidence Checks
- distinguishes repo facts, issue-thread anecdotes, and inference
- proposes next checks tied to evidence gaps
- avoids pretending a fix is verified when only workarounds exist

### Expected Failure Modes
- unsupported root-cause certainty
- generic troubleshooting with no repo-specific evidence
- issue comments treated as canonical documentation

### Rubric Dimensions
- 1. User intent fit: blocked: runtime unavailable
- 2. Direct answer usefulness: blocked: runtime unavailable
- 3. Evidence/claim traceability: blocked: runtime unavailable
- 4. Source quality and diversity: blocked: runtime unavailable
- 5. Official/rumor/opinion separation: blocked: runtime unavailable
- 6. Handling of uncertainty and conflicts: blocked: runtime unavailable
- 7. Information resolution compared with available sources: blocked: runtime unavailable
- 8. Practical next steps or decision support: blocked: runtime unavailable
- 9. Structure and readability: blocked: runtime unavailable

### Critical Failure Flags
- Unsupported factual claim that affects the conclusion
- Fabricated source, URL, date, price, or named entity
- Fails to answer the user's actual question
- Treats rumor/opinion as official fact
- Ignores a major constraint from the prompt
- Produces final answer without meaningful evidence or uncertainty handling

### Artifact And Diagnostic Availability
- source diagnostics envelope: blocked
- controller artifacts envelope: blocked
- structured context packing diagnostics: blocked
