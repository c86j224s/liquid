# Research Richness Run: intermediate-structured-fixture-20260515T080130Z

- Timestamp: 2026-05-15T08:01:33.376606+00:00
- Mode: fixture (deterministic)
- Engine: Fixture Controller
- Model: fixture-research-bench
- Structured scoring: authoritative machine rubric from persisted benchmark artifacts
- Fixture note: Fixture-only pipeline evidence; not live research quality proof.
- JSON aggregate: intermediate-structured-fixture-20260515T080130Z.json
- CSV rows: intermediate-structured-fixture-20260515T080130Z.csv
- NDJSON rows: intermediate-structured-fixture-20260515T080130Z.ndjson

## Aggregate Structured Summary
- Cases: 7
- Completed: 7
- Quality passed: 7
- Critical failure cases: 0 (none)
- Overall score range: 4.89 to 4.89 (avg 4.89)
- Source pack statuses: success=7
- Quality statuses: passed=7

## Dimension Averages
| Dimension | Avg | Min | Max | Cases |
| --- | --- | --- | --- | --- |
| User intent fit | 5.00 | 5 | 5 | 7 |
| Direct answer usefulness | 5.00 | 5 | 5 | 7 |
| Evidence/claim traceability | 5.00 | 5 | 5 | 7 |
| Source quality and diversity | 4.00 | 4 | 4 | 7 |
| Official/rumor/opinion separation | 5.00 | 5 | 5 | 7 |
| Handling of uncertainty and conflicts | 5.00 | 5 | 5 | 7 |
| Information resolution compared with available sources | 5.00 | 5 | 5 | 7 |
| Practical next steps or decision support | 5.00 | 5 | 5 | 7 |
| Structure and readability | 5.00 | 5 | 5 | 7 |

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
- Structured overall score: 4.89/5.00
- Critical failure flags: 0
- Structured visibility: task=completed, quality=passed, source_pack=success
- Measurement kind: pipeline_evidence
- Measurement caveat: Fixture-only pipeline evidence; not live research quality proof.
- Structured summary JSON: not included in sanitized snapshot
- Final output: docs/experiments/research-richness/runs/intermediate-structured-fixture-20260515T080130Z/1-01-product-decision-final-output.md
- Source diagnostics JSON: not included in sanitized snapshot
- Controller artifacts JSON: not included in sanitized snapshot
- Resolved system prompt: not included in sanitized snapshot
- Resolved user prompt: not included in sanitized snapshot

### Rubric Dimensions
- User intent fit: 5/5 [machine_readable_benchmark_artifacts; prompt term hits 20/20 for case 01-product-decision with quality status passed]
- Direct answer usefulness: 5/5 [machine_readable_benchmark_artifacts; final output chars=2725 visible final answer=true task status=completed]
- Evidence/claim traceability: 5/5 [machine_readable_benchmark_artifacts; supported claims=7 total claims=7 unsupported_claim_count=0]
- Source quality and diversity: 4/5 [machine_readable_benchmark_artifacts; source_cards=7 distinct_hosts=3 official_sources=7 adopted_sources=7]
- Official/rumor/opinion separation: 5/5 [machine_readable_benchmark_artifacts; official_sources=7 rumor_or_opinion_sources=0 unsupported_claim_count=0]
- Handling of uncertainty and conflicts: 5/5 [machine_readable_benchmark_artifacts; conflicts=1 resolved=1 promoted_to_debt=0 open_debt=0 next_actions=2]
- Information resolution compared with available sources: 5/5 [machine_readable_benchmark_artifacts; adopted_sources=7 source_cards=7 claims=7 context_included_sources=3]
- Practical next steps or decision support: 5/5 [machine_readable_benchmark_artifacts; decision_terms=true open_debt=0 next_actions=2 prompt_hits=20/20]
- Structure and readability: 5/5 [machine_readable_benchmark_artifacts; sections present: final_answer=true source_audit=true claim_log=true quality_gate=true appendix=true]

### Critical Failure Flags
- Unsupported factual claim that affects the conclusion: clear (quality gate unsupported_claim_count=0)
- Fabricated source, URL, date, price, or named entity: clear (invalid source card URL count=0)
- Fails to answer the user's actual question: clear (visible final answer section present=true)
- Treats rumor/opinion as official fact: clear (rumor/opinion sources=0 official sources=7 quality_status=passed)
- Ignores a major constraint from the prompt: clear (prompt term hits=20 threshold=2 for case 01-product-decision)
- Produces final answer without meaningful evidence or uncertainty handling: clear (visible_final_answer=true source_cards=7 supported_claims=7 claim_log_visible=true)

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
- fixture-only pipeline evidence label: Fixture-only pipeline evidence; not live research quality proof.


## Local Recommendation With Constraints

- Category: local-recommendation
- Case file: 02-local-recommendation.md
- Prompt: Recommend a quiet Seoul coffee shop for a two-hour weekday work session near Line 2 with power outlets, light food, and no long detour from transit.
- Task status: completed
- Quality status: passed
- Quality last failure: none
- Structured overall score: 4.89/5.00
- Critical failure flags: 0
- Structured visibility: task=completed, quality=passed, source_pack=success
- Measurement kind: pipeline_evidence
- Measurement caveat: Fixture-only pipeline evidence; not live research quality proof.
- Structured summary JSON: not included in sanitized snapshot
- Final output: docs/experiments/research-richness/runs/intermediate-structured-fixture-20260515T080130Z/2-02-local-recommendation-final-output.md
- Source diagnostics JSON: not included in sanitized snapshot
- Controller artifacts JSON: not included in sanitized snapshot
- Resolved system prompt: not included in sanitized snapshot
- Resolved user prompt: not included in sanitized snapshot

### Rubric Dimensions
- User intent fit: 5/5 [machine_readable_benchmark_artifacts; prompt term hits 21/21 for case 02-local-recommendation with quality status passed]
- Direct answer usefulness: 5/5 [machine_readable_benchmark_artifacts; final output chars=2672 visible final answer=true task status=completed]
- Evidence/claim traceability: 5/5 [machine_readable_benchmark_artifacts; supported claims=7 total claims=7 unsupported_claim_count=0]
- Source quality and diversity: 4/5 [machine_readable_benchmark_artifacts; source_cards=7 distinct_hosts=3 official_sources=7 adopted_sources=7]
- Official/rumor/opinion separation: 5/5 [machine_readable_benchmark_artifacts; official_sources=7 rumor_or_opinion_sources=0 unsupported_claim_count=0]
- Handling of uncertainty and conflicts: 5/5 [machine_readable_benchmark_artifacts; conflicts=1 resolved=1 promoted_to_debt=0 open_debt=0 next_actions=2]
- Information resolution compared with available sources: 5/5 [machine_readable_benchmark_artifacts; adopted_sources=7 source_cards=7 claims=7 context_included_sources=3]
- Practical next steps or decision support: 5/5 [machine_readable_benchmark_artifacts; decision_terms=true open_debt=0 next_actions=2 prompt_hits=21/21]
- Structure and readability: 5/5 [machine_readable_benchmark_artifacts; sections present: final_answer=true source_audit=true claim_log=true quality_gate=true appendix=true]

### Critical Failure Flags
- Unsupported factual claim that affects the conclusion: clear (quality gate unsupported_claim_count=0)
- Fabricated source, URL, date, price, or named entity: clear (invalid source card URL count=0)
- Fails to answer the user's actual question: clear (visible final answer section present=true)
- Treats rumor/opinion as official fact: clear (rumor/opinion sources=0 official sources=7 quality_status=passed)
- Ignores a major constraint from the prompt: clear (prompt term hits=21 threshold=2 for case 02-local-recommendation)
- Produces final answer without meaningful evidence or uncertainty handling: clear (visible_final_answer=true source_cards=7 supported_claims=7 claim_log_visible=true)

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
- fixture-only pipeline evidence label: Fixture-only pipeline evidence; not live research quality proof.


## Historical Chronology And Causality

- Category: historical-explanation
- Case file: 03-historical-explanation.md
- Prompt: Explain why the Gothic War under Justinian expanded, why it lasted so long, and what consequences followed in Italy, with chronology and contested points separated clearly.
- Task status: completed
- Quality status: passed
- Quality last failure: none
- Structured overall score: 4.89/5.00
- Critical failure flags: 0
- Structured visibility: task=completed, quality=passed, source_pack=success
- Measurement kind: pipeline_evidence
- Measurement caveat: Fixture-only pipeline evidence; not live research quality proof.
- Structured summary JSON: not included in sanitized snapshot
- Final output: docs/experiments/research-richness/runs/intermediate-structured-fixture-20260515T080130Z/3-03-historical-explanation-final-output.md
- Source diagnostics JSON: not included in sanitized snapshot
- Controller artifacts JSON: not included in sanitized snapshot
- Resolved system prompt: not included in sanitized snapshot
- Resolved user prompt: not included in sanitized snapshot

### Rubric Dimensions
- User intent fit: 5/5 [machine_readable_benchmark_artifacts; prompt term hits 19/19 for case 03-historical-explanation with quality status passed]
- Direct answer usefulness: 5/5 [machine_readable_benchmark_artifacts; final output chars=2696 visible final answer=true task status=completed]
- Evidence/claim traceability: 5/5 [machine_readable_benchmark_artifacts; supported claims=7 total claims=7 unsupported_claim_count=0]
- Source quality and diversity: 4/5 [machine_readable_benchmark_artifacts; source_cards=7 distinct_hosts=3 official_sources=7 adopted_sources=7]
- Official/rumor/opinion separation: 5/5 [machine_readable_benchmark_artifacts; official_sources=7 rumor_or_opinion_sources=0 unsupported_claim_count=0]
- Handling of uncertainty and conflicts: 5/5 [machine_readable_benchmark_artifacts; conflicts=1 resolved=1 promoted_to_debt=0 open_debt=0 next_actions=2]
- Information resolution compared with available sources: 5/5 [machine_readable_benchmark_artifacts; adopted_sources=7 source_cards=7 claims=7 context_included_sources=3]
- Practical next steps or decision support: 5/5 [machine_readable_benchmark_artifacts; decision_terms=true open_debt=0 next_actions=2 prompt_hits=19/19]
- Structure and readability: 5/5 [machine_readable_benchmark_artifacts; sections present: final_answer=true source_audit=true claim_log=true quality_gate=true appendix=true]

### Critical Failure Flags
- Unsupported factual claim that affects the conclusion: clear (quality gate unsupported_claim_count=0)
- Fabricated source, URL, date, price, or named entity: clear (invalid source card URL count=0)
- Fails to answer the user's actual question: clear (visible final answer section present=true)
- Treats rumor/opinion as official fact: clear (rumor/opinion sources=0 official sources=7 quality_status=passed)
- Ignores a major constraint from the prompt: clear (prompt term hits=19 threshold=2 for case 03-historical-explanation)
- Produces final answer without meaningful evidence or uncertainty handling: clear (visible_final_answer=true source_cards=7 supported_claims=7 claim_log_visible=true)

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
- fixture-only pipeline evidence label: Fixture-only pipeline evidence; not live research quality proof.


## Weak-Source Rumor And Unofficial Claim

- Category: weak-source-rumor
- Case file: 04-weak-source-rumor.md
- Prompt: Assess whether a rumored unreleased device feature is likely, what evidence exists, and what should still be treated as rumor versus official information.
- Task status: completed
- Quality status: passed
- Quality last failure: none
- Structured overall score: 4.89/5.00
- Critical failure flags: 0
- Structured visibility: task=completed, quality=passed, source_pack=success
- Measurement kind: pipeline_evidence
- Measurement caveat: Fixture-only pipeline evidence; not live research quality proof.
- Structured summary JSON: not included in sanitized snapshot
- Final output: docs/experiments/research-richness/runs/intermediate-structured-fixture-20260515T080130Z/4-04-weak-source-rumor-final-output.md
- Source diagnostics JSON: not included in sanitized snapshot
- Controller artifacts JSON: not included in sanitized snapshot
- Resolved system prompt: not included in sanitized snapshot
- Resolved user prompt: not included in sanitized snapshot

### Rubric Dimensions
- User intent fit: 5/5 [machine_readable_benchmark_artifacts; prompt term hits 16/16 for case 04-weak-source-rumor with quality status passed]
- Direct answer usefulness: 5/5 [machine_readable_benchmark_artifacts; final output chars=2676 visible final answer=true task status=completed]
- Evidence/claim traceability: 5/5 [machine_readable_benchmark_artifacts; supported claims=7 total claims=7 unsupported_claim_count=0]
- Source quality and diversity: 4/5 [machine_readable_benchmark_artifacts; source_cards=7 distinct_hosts=3 official_sources=7 adopted_sources=7]
- Official/rumor/opinion separation: 5/5 [machine_readable_benchmark_artifacts; official_sources=7 rumor_or_opinion_sources=0 unsupported_claim_count=0]
- Handling of uncertainty and conflicts: 5/5 [machine_readable_benchmark_artifacts; conflicts=1 resolved=1 promoted_to_debt=0 open_debt=0 next_actions=2]
- Information resolution compared with available sources: 5/5 [machine_readable_benchmark_artifacts; adopted_sources=7 source_cards=7 claims=7 context_included_sources=3]
- Practical next steps or decision support: 5/5 [machine_readable_benchmark_artifacts; decision_terms=true open_debt=0 next_actions=2 prompt_hits=16/16]
- Structure and readability: 5/5 [machine_readable_benchmark_artifacts; sections present: final_answer=true source_audit=true claim_log=true quality_gate=true appendix=true]

### Critical Failure Flags
- Unsupported factual claim that affects the conclusion: clear (quality gate unsupported_claim_count=0)
- Fabricated source, URL, date, price, or named entity: clear (invalid source card URL count=0)
- Fails to answer the user's actual question: clear (visible final answer section present=true)
- Treats rumor/opinion as official fact: clear (rumor/opinion sources=0 official sources=7 quality_status=passed)
- Ignores a major constraint from the prompt: clear (prompt term hits=16 threshold=2 for case 04-weak-source-rumor)
- Produces final answer without meaningful evidence or uncertainty handling: clear (visible_final_answer=true source_cards=7 supported_claims=7 claim_log_visible=true)

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
- fixture-only pipeline evidence label: Fixture-only pipeline evidence; not live research quality proof.


## Conflicting Current-Events Evidence

- Category: conflicting-current-events
- Case file: 05-conflicting-current-events.md
- Prompt: Summarize a fast-moving current event where early reports conflict, identify what is verified, and list what remains open.
- Task status: completed
- Quality status: passed
- Quality last failure: none
- Structured overall score: 4.89/5.00
- Critical failure flags: 0
- Structured visibility: task=completed, quality=passed, source_pack=success
- Measurement kind: pipeline_evidence
- Measurement caveat: Fixture-only pipeline evidence; not live research quality proof.
- Structured summary JSON: not included in sanitized snapshot
- Final output: docs/experiments/research-richness/runs/intermediate-structured-fixture-20260515T080130Z/5-05-conflicting-current-events-final-output.md
- Source diagnostics JSON: not included in sanitized snapshot
- Controller artifacts JSON: not included in sanitized snapshot
- Resolved system prompt: not included in sanitized snapshot
- Resolved user prompt: not included in sanitized snapshot

### Rubric Dimensions
- User intent fit: 5/5 [machine_readable_benchmark_artifacts; prompt term hits 13/13 for case 05-conflicting-current-events with quality status passed]
- Direct answer usefulness: 5/5 [machine_readable_benchmark_artifacts; final output chars=2650 visible final answer=true task status=completed]
- Evidence/claim traceability: 5/5 [machine_readable_benchmark_artifacts; supported claims=7 total claims=7 unsupported_claim_count=0]
- Source quality and diversity: 4/5 [machine_readable_benchmark_artifacts; source_cards=7 distinct_hosts=3 official_sources=7 adopted_sources=7]
- Official/rumor/opinion separation: 5/5 [machine_readable_benchmark_artifacts; official_sources=7 rumor_or_opinion_sources=0 unsupported_claim_count=0]
- Handling of uncertainty and conflicts: 5/5 [machine_readable_benchmark_artifacts; conflicts=1 resolved=1 promoted_to_debt=0 open_debt=0 next_actions=2]
- Information resolution compared with available sources: 5/5 [machine_readable_benchmark_artifacts; adopted_sources=7 source_cards=7 claims=7 context_included_sources=3]
- Practical next steps or decision support: 5/5 [machine_readable_benchmark_artifacts; decision_terms=true open_debt=0 next_actions=2 prompt_hits=13/13]
- Structure and readability: 5/5 [machine_readable_benchmark_artifacts; sections present: final_answer=true source_audit=true claim_log=true quality_gate=true appendix=true]

### Critical Failure Flags
- Unsupported factual claim that affects the conclusion: clear (quality gate unsupported_claim_count=0)
- Fabricated source, URL, date, price, or named entity: clear (invalid source card URL count=0)
- Fails to answer the user's actual question: clear (visible final answer section present=true)
- Treats rumor/opinion as official fact: clear (rumor/opinion sources=0 official sources=7 quality_status=passed)
- Ignores a major constraint from the prompt: clear (prompt term hits=13 threshold=2 for case 05-conflicting-current-events)
- Produces final answer without meaningful evidence or uncertainty handling: clear (visible_final_answer=true source_cards=7 supported_claims=7 claim_log_visible=true)

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
- fixture-only pipeline evidence label: Fixture-only pipeline evidence; not live research quality proof.


## Numerical Comparison With Dates Prices Or Versions

- Category: numeric-comparison
- Case file: 06-numeric-comparison.md
- Prompt: Compare two software hosting plans using current published prices, rate limits, and feature tiers, including the exact date context for each figure.
- Task status: completed
- Quality status: passed
- Quality last failure: none
- Structured overall score: 4.89/5.00
- Critical failure flags: 0
- Structured visibility: task=completed, quality=passed, source_pack=success
- Measurement kind: pipeline_evidence
- Measurement caveat: Fixture-only pipeline evidence; not live research quality proof.
- Structured summary JSON: not included in sanitized snapshot
- Final output: docs/experiments/research-richness/runs/intermediate-structured-fixture-20260515T080130Z/6-06-numeric-comparison-final-output.md
- Source diagnostics JSON: not included in sanitized snapshot
- Controller artifacts JSON: not included in sanitized snapshot
- Resolved system prompt: not included in sanitized snapshot
- Resolved user prompt: not included in sanitized snapshot

### Rubric Dimensions
- User intent fit: 5/5 [machine_readable_benchmark_artifacts; prompt term hits 18/18 for case 06-numeric-comparison with quality status passed]
- Direct answer usefulness: 5/5 [machine_readable_benchmark_artifacts; final output chars=2683 visible final answer=true task status=completed]
- Evidence/claim traceability: 5/5 [machine_readable_benchmark_artifacts; supported claims=7 total claims=7 unsupported_claim_count=0]
- Source quality and diversity: 4/5 [machine_readable_benchmark_artifacts; source_cards=7 distinct_hosts=3 official_sources=7 adopted_sources=7]
- Official/rumor/opinion separation: 5/5 [machine_readable_benchmark_artifacts; official_sources=7 rumor_or_opinion_sources=0 unsupported_claim_count=0]
- Handling of uncertainty and conflicts: 5/5 [machine_readable_benchmark_artifacts; conflicts=1 resolved=1 promoted_to_debt=0 open_debt=0 next_actions=2]
- Information resolution compared with available sources: 5/5 [machine_readable_benchmark_artifacts; adopted_sources=7 source_cards=7 claims=7 context_included_sources=3]
- Practical next steps or decision support: 5/5 [machine_readable_benchmark_artifacts; decision_terms=true open_debt=0 next_actions=2 prompt_hits=18/18]
- Structure and readability: 5/5 [machine_readable_benchmark_artifacts; sections present: final_answer=true source_audit=true claim_log=true quality_gate=true appendix=true]

### Critical Failure Flags
- Unsupported factual claim that affects the conclusion: clear (quality gate unsupported_claim_count=0)
- Fabricated source, URL, date, price, or named entity: clear (invalid source card URL count=0)
- Fails to answer the user's actual question: clear (visible final answer section present=true)
- Treats rumor/opinion as official fact: clear (rumor/opinion sources=0 official sources=7 quality_status=passed)
- Ignores a major constraint from the prompt: clear (prompt term hits=18 threshold=2 for case 06-numeric-comparison)
- Produces final answer without meaningful evidence or uncertainty handling: clear (visible_final_answer=true source_cards=7 supported_claims=7 claim_log_visible=true)

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
- fixture-only pipeline evidence label: Fixture-only pipeline evidence; not live research quality proof.


## Sparse Technical Troubleshooting

- Category: niche-troubleshooting
- Case file: 07-niche-troubleshooting.md
- Prompt: Diagnose a niche local-model tool-calling failure in an open-source repo where documentation is sparse, and recommend the safest next debugging steps.
- Task status: completed
- Quality status: passed
- Quality last failure: none
- Structured overall score: 4.89/5.00
- Critical failure flags: 0
- Structured visibility: task=completed, quality=passed, source_pack=success
- Measurement kind: pipeline_evidence
- Measurement caveat: Fixture-only pipeline evidence; not live research quality proof.
- Structured summary JSON: not included in sanitized snapshot
- Final output: docs/experiments/research-richness/runs/intermediate-structured-fixture-20260515T080130Z/7-07-niche-troubleshooting-final-output.md
- Source diagnostics JSON: not included in sanitized snapshot
- Controller artifacts JSON: not included in sanitized snapshot
- Resolved system prompt: not included in sanitized snapshot
- Resolved user prompt: not included in sanitized snapshot

### Rubric Dimensions
- User intent fit: 5/5 [machine_readable_benchmark_artifacts; prompt term hits 19/19 for case 07-niche-troubleshooting with quality status passed]
- Direct answer usefulness: 5/5 [machine_readable_benchmark_artifacts; final output chars=2670 visible final answer=true task status=completed]
- Evidence/claim traceability: 5/5 [machine_readable_benchmark_artifacts; supported claims=7 total claims=7 unsupported_claim_count=0]
- Source quality and diversity: 4/5 [machine_readable_benchmark_artifacts; source_cards=7 distinct_hosts=3 official_sources=7 adopted_sources=7]
- Official/rumor/opinion separation: 5/5 [machine_readable_benchmark_artifacts; official_sources=7 rumor_or_opinion_sources=0 unsupported_claim_count=0]
- Handling of uncertainty and conflicts: 5/5 [machine_readable_benchmark_artifacts; conflicts=1 resolved=1 promoted_to_debt=0 open_debt=0 next_actions=2]
- Information resolution compared with available sources: 5/5 [machine_readable_benchmark_artifacts; adopted_sources=7 source_cards=7 claims=7 context_included_sources=3]
- Practical next steps or decision support: 5/5 [machine_readable_benchmark_artifacts; decision_terms=true open_debt=0 next_actions=2 prompt_hits=19/19]
- Structure and readability: 5/5 [machine_readable_benchmark_artifacts; sections present: final_answer=true source_audit=true claim_log=true quality_gate=true appendix=true]

### Critical Failure Flags
- Unsupported factual claim that affects the conclusion: clear (quality gate unsupported_claim_count=0)
- Fabricated source, URL, date, price, or named entity: clear (invalid source card URL count=0)
- Fails to answer the user's actual question: clear (visible final answer section present=true)
- Treats rumor/opinion as official fact: clear (rumor/opinion sources=0 official sources=7 quality_status=passed)
- Ignores a major constraint from the prompt: clear (prompt term hits=19 threshold=2 for case 07-niche-troubleshooting)
- Produces final answer without meaningful evidence or uncertainty handling: clear (visible_final_answer=true source_cards=7 supported_claims=7 claim_log_visible=true)

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
- fixture-only pipeline evidence label: Fixture-only pipeline evidence; not live research quality proof.
