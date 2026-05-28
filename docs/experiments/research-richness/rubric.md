# Research Richness Rubric

Restated here from the original internal research-controller benchmark so this sanitized snapshot stays self-contained.

Each dimension is scored 0-5.

Common dimensions stay unchanged across categories. The benchmark also adds one category-aware richness dimension on top of the shared 9 dimensions. Some categories may add benchmark-only overlay gates in structured outputs; these overlays can trigger critical failures or cap the overall score without changing the shared 9-dimension rubric.

Historical benchmark categories (`historical-research`, `historical-explanation`) add a benchmark-only overlay. The overlay checks for chronology or period context, actors and geography, causes and consequences, and evidence/scope limits or contested interpretation. If one historical overlay gate triggers, the overall score is capped at 2.5; if two or more trigger, the overall score is capped at 2.0.

For Hannibal / Second Punic War-class historical cases, there is now an additive visible phase-density floor on top of the generic historical overlay. This is structural only, not prose-copying: the reader-facing answer must keep a long enough campaign narrative, multiple phase subsections, repeated date anchors, and repeated Hannibal/Rome/Carthage/front anchors. Flat significance summaries should fail even if the appendix artifacts look clean.

The added `Genre/section richness` dimension is category-aware. Historical categories reward visible body structure such as contemporary comparison, chronology-vs-interpretation separation, source/evidence layers, issue mapping, later impact, and follow-up exploration questions. Technology-oriented categories reward visible design judgment, trade-off handling, and verifiability. This dimension is additive; it does not replace any of the common 9 dimensions.

For high-intensity strict historical runs, hidden `narrative_state` or `reader_quality` artifacts are still diagnostic-only, not evidence, but they are now expected to carry useful planning depth. A historical run that keeps both empty should surface an explicit repair failure rather than pass as a shallow but clean-looking report.

Reader Quality metrics are observable-but-unscored in this phase. The benchmark records additive counts for argument-graph nodes/edges, narrative-plan presence, section-brief coverage, and reader-critique metrics, but these do not change rubric weights or caps yet. In historical strict/high work they are expected as hidden planning diagnostics, not as evidence.

| Score | Meaning |
| --- | --- |
| 0 | Missing or actively wrong |
| 1 | Present but shallow, generic, or unreliable |
| 2 | Partially useful with important gaps |
| 3 | Adequate, but not clearly better than current flow |
| 4 | Strong and visibly better than current flow |
| 5 | Excellent, decision-grade, and well supported |

## Dimensions

1. User intent fit
2. Direct answer usefulness
3. Evidence/claim traceability
4. Source quality and diversity
5. Official/rumor/opinion separation
6. Handling of uncertainty and conflicts
7. Information resolution compared with available sources
8. Practical next steps or decision support
9. Structure and readability
10. Genre/section richness (category-aware benchmark-only dimension)

## Artifact Notes

- Source Cards: URLs, source class, extracted facts, limitation, confidence
- Claim Log: claim IDs, support references, uncertainty notes, verification status
- Conflict Map: conflicting claim IDs, resolution state, debt promotion
- Research Debt: failed gate, missing evidence, required source class, candidate queries, next actions
- Historical open debt must be specific about the missing phase, actor, place/front, transition, source layer, or interpretive gap; generic placeholders such as `missing evidence not specified` are not acceptable
- Diagnostics: scrape outcome, extraction strategy, size counts, source-pack adopted/skipped candidates, packing diagnostics

## Critical Failure Flags

- Unsupported factual claim that affects the conclusion
- Fabricated source, URL, date, price, or named entity
- Fails to answer the user's actual question
- Treats rumor/opinion as official fact
- Ignores a major constraint from the prompt
- Produces final answer without meaningful evidence or uncertainty handling
- Historical-category overlay may additionally fail shallow history that omits chronology, actors/geography, causes/consequences, or evidence/scope limits, even if the common dimensions average well
- Hannibal / Second Punic War-class cases may additionally fail if visible campaign phase density collapses below the structural floor, even when generic historical richness markers are present

## Promising Threshold

- Average score >= 3.8
- No critical failure flag
- Evidence-backed claims are distinguishable from inference
- Critic section identifies meaningful remaining weaknesses or explicitly explains why none remain
