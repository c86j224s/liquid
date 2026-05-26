# Research Richness Run: live-source-collection-20260516T151509Z

- Timestamp: 2026-05-16T15:15:16.088182+00:00
- Mode: live (non-deterministic)
- Engine: Headless Research
- Model: cli:codex
- Structured scoring: authoritative machine rubric from persisted benchmark artifacts
- Live note: Live non-deterministic benchmark measurement; compare only against runs with matching provider/runtime settings.
- JSON aggregate: live-source-collection-20260516T151509Z.json
- CSV rows: live-source-collection-20260516T151509Z.csv
- NDJSON rows: live-source-collection-20260516T151509Z.ndjson

## Aggregate Structured Summary
- Cases: 2
- Completed: 2
- Quality passed: 1
- Critical failure cases: 1 (01-aurelian-source-collection)
- Overall score range: 2.00 to 4.60 (avg 3.30)
- Source pack statuses: success=2
- Quality statuses: passed=1, untrusted=1

## Dimension Averages
| Dimension | Avg | Min | Max | Cases |
| --- | --- | --- | --- | --- |
| User intent fit | 4.50 | 4 | 5 | 2 |
| Direct answer usefulness | 5.00 | 5 | 5 | 2 |
| Evidence/claim traceability | 5.00 | 5 | 5 | 2 |
| Source quality and diversity | 4.00 | 3 | 5 | 2 |
| Official/rumor/opinion separation | 4.00 | 3 | 5 | 2 |
| Handling of uncertainty and conflicts | 5.00 | 5 | 5 | 2 |
| Information resolution compared with available sources | 5.00 | 5 | 5 | 2 |
| Practical next steps or decision support | 5.00 | 5 | 5 | 2 |
| Structure and readability | 5.00 | 5 | 5 | 2 |
| Genre/section richness | 2.00 | 2 | 2 | 2 |

## Case Inventory
- historical-research: Aurelian Source Collection And Reliability (01-aurelian-source-collection.md)
- niche-troubleshooting: C++ Work-Stealing Scheduler Research Sample (02-cpp-work-stealing-scheduler.md)

## Aurelian Source Collection And Reliability

- Category: historical-research
- Case file: 01-aurelian-source-collection.md
- Prompt: Research Emperor Aurelian's reign with emphasis on chronology, military and political reforms, source reliability, and the limits of later or contested ancient traditions such as the Historia Augusta. Explain what can be treated as relatively secure, what remains debated, and how weak or problematic sources should be used without anchoring decisive conclusions.
- Task status: completed
- Quality status: passed
- Quality last failure: none
- Replay before: not applicable
- Structured overall score: 2.00/5.00
- Critical failure flags: 3
- Structured visibility: task=completed, quality=passed, source_pack=success
- Measurement kind: live_non_deterministic_quality_measurement
- Measurement caveat: Live non-deterministic benchmark measurement; compare only against runs with matching provider/runtime settings.
- Structured summary JSON: not included in sanitized snapshot
- Final output: docs/experiments/research-richness/runs/live-source-collection-20260516T151509Z/1-01-aurelian-source-collection-final-output.md
- Source diagnostics JSON: not included in sanitized snapshot
- Controller artifacts JSON: not included in sanitized snapshot
- Resolved system prompt: not included in sanitized snapshot
- Resolved user prompt: not included in sanitized snapshot

### Rubric Dimensions
- User intent fit: 5/5 [machine_readable_benchmark_artifacts; prompt term hits 29/37 for case 01-aurelian-source-collection with quality status passed]
- Direct answer usefulness: 5/5 [machine_readable_benchmark_artifacts; final output chars=15591 visible final answer=true task status=completed]
- Evidence/claim traceability: 5/5 [machine_readable_benchmark_artifacts; supported claims=10 total claims=10 unsupported_claim_count=0]
- Source quality and diversity: 3/5 [machine_readable_benchmark_artifacts; source_cards=17 distinct_hosts=9 official_sources=0 adopted_sources=4]
- Official/rumor/opinion separation: 3/5 [machine_readable_benchmark_artifacts; official_sources=0 rumor_or_opinion_sources=5 unsupported_claim_count=0]
- Handling of uncertainty and conflicts: 5/5 [machine_readable_benchmark_artifacts; conflicts=4 resolved=0 promoted_to_debt=4 open_debt=6 next_actions=9]
- Information resolution compared with available sources: 5/5 [machine_readable_benchmark_artifacts; adopted_sources=4 source_cards=17 claims=10 context_included_sources=0]
- Practical next steps or decision support: 5/5 [machine_readable_benchmark_artifacts; decision_terms=true open_debt=6 next_actions=9 prompt_hits=29/37]
- Structure and readability: 5/5 [machine_readable_benchmark_artifacts; sections present: final_answer=true source_audit=true claim_log=true quality_gate=true appendix=true reader_facing_prompt_echo_count=0 reader_facing_artifact_leak_count=0]
- Genre/section richness: 2/5 [machine_readable_benchmark_artifacts; historical richness coverage=2 comparison=1 chronology_interpretation=0 source_layers=0 issue_map=1 legacy=0 follow_up=0]

### Critical Failure Flags
- Unsupported factual claim that affects the conclusion: clear (quality gate unsupported_claim_count=0)
- Fabricated source, URL, date, price, or named entity: clear (invalid source card URL count=0)
- Fails to answer the user's actual question: clear (visible final answer section present=true)
- Treats rumor/opinion as official fact: triggered (rumor/opinion sources=5 official sources=0 quality_status=passed)
- Ignores a major constraint from the prompt: clear (prompt term hits=29 threshold=2 for case 01-aurelian-source-collection)
- Produces final answer without meaningful evidence or uncertainty handling: clear (visible_final_answer=true source_cards=17 supported_claims=10 claim_log_visible=true)
- Reader-facing final answer leaks prompt/controller-validation artifacts: clear (reader_facing_prompt_echo_count=0 reader_facing_artifact_leak_count=0)
- Historical answer omits clear sequence or period context: clear (historical chronology signals=3)
- Historical answer omits key actors or geographic scope: clear (historical actor signals=2 geography signals=1)
- Historical answer omits causal explanation or consequences: triggered (historical cause signals=0 consequence signals=1)
- Historical answer omits source limits, scope limits, or contested interpretations: triggered (historical evidence_limit signals=1 contested signals=0 scope_limit signals=0)

### Must-Pass Evidence Checks
- distinguishes stronger modern scholarship or reference works from weak/problematic ancient traditions
- uses Historia Augusta-style material only as supplementary or contested context
- includes chronology, actors, reforms, constraints, and consequences
- explains uncertainty and source limits without collapsing into a short summary

### Expected Failure Modes
- treats weak ancient traditions as decisive evidence
- provides report-like prose but too little factual volume
- omits source criticism or contested interpretations
- states a clean conclusion without showing evidence confidence

### Artifact And Diagnostic Availability
- source diagnostics envelope: present
- controller artifacts envelope: present
- structured context packing diagnostics: present
- fixture-only pipeline evidence label: not applicable


## C++ Work-Stealing Scheduler Research Sample

- Category: niche-troubleshooting
- Case file: 02-cpp-work-stealing-scheduler.md
- Prompt: Research how to implement a work-stealing scheduler in modern C++. Cover the core architecture, deque ownership model, stealing protocol, memory ordering concerns, task representation, shutdown/cancellation, exception handling, benchmarking, and implementation pitfalls. Distinguish widely accepted design patterns from implementation choices and call out what should be verified with tests or benchmarks.
- Task status: completed
- Quality status: untrusted
- Quality last failure: Research quality gate failed: authoritative evidence URL count 2 is below required minimum 5
- Replay before: not applicable
- Structured overall score: 4.60/5.00
- Critical failure flags: 0
- Structured visibility: task=completed, quality=untrusted, source_pack=success
- Measurement kind: live_non_deterministic_quality_measurement
- Measurement caveat: Live non-deterministic benchmark measurement; compare only against runs with matching provider/runtime settings.
- Structured summary JSON: not included in sanitized snapshot
- Final output: docs/experiments/research-richness/runs/live-source-collection-20260516T151509Z/2-02-cpp-work-stealing-scheduler-final-output.md
- Source diagnostics JSON: not included in sanitized snapshot
- Controller artifacts JSON: not included in sanitized snapshot
- Resolved system prompt: not included in sanitized snapshot
- Resolved user prompt: not included in sanitized snapshot

### Rubric Dimensions
- User intent fit: 4/5 [machine_readable_benchmark_artifacts; prompt term hits 33/39 for case 02-cpp-work-stealing-scheduler with quality status untrusted]
- Direct answer usefulness: 5/5 [machine_readable_benchmark_artifacts; final output chars=17238 visible final answer=true task status=completed]
- Evidence/claim traceability: 5/5 [machine_readable_benchmark_artifacts; supported claims=14 total claims=14 unsupported_claim_count=0]
- Source quality and diversity: 5/5 [machine_readable_benchmark_artifacts; source_cards=16 distinct_hosts=10 official_sources=6 adopted_sources=8]
- Official/rumor/opinion separation: 5/5 [machine_readable_benchmark_artifacts; official_sources=6 rumor_or_opinion_sources=0 unsupported_claim_count=0]
- Handling of uncertainty and conflicts: 5/5 [machine_readable_benchmark_artifacts; conflicts=2 resolved=0 promoted_to_debt=2 open_debt=4 next_actions=24]
- Information resolution compared with available sources: 5/5 [machine_readable_benchmark_artifacts; adopted_sources=8 source_cards=16 claims=14 context_included_sources=8]
- Practical next steps or decision support: 5/5 [machine_readable_benchmark_artifacts; decision_terms=true open_debt=4 next_actions=24 prompt_hits=33/39]
- Structure and readability: 5/5 [machine_readable_benchmark_artifacts; sections present: final_answer=true source_audit=true claim_log=true quality_gate=true appendix=true reader_facing_prompt_echo_count=0 reader_facing_artifact_leak_count=0]
- Genre/section richness: 2/5 [machine_readable_benchmark_artifacts; technology richness coverage=1 design_judgment=0 tradeoff=0 verifiability=1]

### Critical Failure Flags
- Unsupported factual claim that affects the conclusion: clear (quality gate unsupported_claim_count=0)
- Fabricated source, URL, date, price, or named entity: clear (invalid source card URL count=0)
- Fails to answer the user's actual question: clear (visible final answer section present=true)
- Treats rumor/opinion as official fact: clear (rumor/opinion sources=0 official sources=6 quality_status=untrusted)
- Ignores a major constraint from the prompt: clear (prompt term hits=33 threshold=2 for case 02-cpp-work-stealing-scheduler)
- Produces final answer without meaningful evidence or uncertainty handling: clear (visible_final_answer=true source_cards=16 supported_claims=14 claim_log_visible=true)
- Reader-facing final answer leaks prompt/controller-validation artifacts: clear (reader_facing_prompt_echo_count=0 reader_facing_artifact_leak_count=0)
- Historical answer omits clear sequence or period context: clear (historical chronology signals=0)
- Historical answer omits key actors or geographic scope: clear (historical actor signals=0 geography signals=0)
- Historical answer omits causal explanation or consequences: clear (historical cause signals=0 consequence signals=0)
- Historical answer omits source limits, scope limits, or contested interpretations: clear (historical evidence_limit signals=0 contested signals=0 scope_limit signals=0)

### Must-Pass Evidence Checks
- explains worker-local deques, steal direction, task ownership, and scheduling invariants
- discusses C++ concurrency and memory ordering risks rather than only high-level concepts
- separates design options, trade-offs, and verification steps
- includes enough detail to guide an implementation plan

### Expected Failure Modes
- generic thread-pool summary without work-stealing specifics
- ignores synchronization, ABA/lifetime, shutdown, or benchmarking risks
- overstates one design as universally correct without trade-offs
- lacks actionable implementation guidance

### Artifact And Diagnostic Availability
- source diagnostics envelope: present
- controller artifacts envelope: present
- structured context packing diagnostics: present
- fixture-only pipeline evidence label: not applicable
