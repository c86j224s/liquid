# Artifact And Diagnostics Schema

## `research_controller_artifacts_json`

Envelope:

```json
{
  "version": 1,
  "events": [],
  "source_cards": [],
  "claim_log": [],
  "conflict_map": [],
  "research_debt": [],
  "reader_quality": {},
  "quality_gate": {},
  "warnings": []
}
```

Fields:

- `events`: controller stage history preserved across retries; `narrative_enrichment` is the bounded post-persistence/pre-finalization stage for additive narrative/artifact enrichment strategies
- `source_cards`: `id`, `url`, `title`, `source_class`, `accessed_at`, `extracted_facts`, `limitation`, `diagnostics_ref`, `confidence`
- `claim_log`: `id`, `claim`, `claim_type`, `support_source_card_ids`, `support_urls`, `confidence`, `uncertainty_note`, `needs_verification`
- `conflict_map`: `id`, `topic`, `conflicting_claim_ids`, `source_card_ids`, `resolution_status`, `resolution_note`, `promoted_to_debt`
- `research_debt`: `id`, `severity`, `failed_gate`, `missing_evidence`, `required_source_class`, `candidate_queries`, `next_check_actions`, `status`
- `narrative_state`: optional outline continuity artifact only; never evidence
- `reader_quality`: optional reader-planning artifact only; never evidence
- `quality_gate`: `status`, `failure_messages`, `unsupported_claim_count`, `unresolved_conflict_count`, `open_debt_count`
- `warnings`: parser, compatibility, or finalization warnings persisted after deterministic repair

Canonical placement:

- `narrative_state.event_cards` is the canonical location for historical/process phase dossiers. Event-card enrichment is a controller sub-pipeline, not only prompt wording: it runs after artifact parsing/persistence and before finalization when a strict/high historical strategy applies.
- Root-level `event_cards`, if a model emits it, is compatibility noise and should not be treated as the trusted scaffold or as evidence.
- Hidden scaffold depth and visible report richness are independent checks: a visible report can read well while failing because the hidden scaffold lacks grounded causal/interpretive structure, and a hidden scaffold cannot compensate for a flat reader-facing final answer.
- Reader Quality metrics are currently observable diagnostics, not weighted score inputs; they explain narrative planning coverage without replacing evidence gates or the genre/section richness dimension.

Finalization notes:

- trusted save uses finalized visible output, not the raw model draft
- the finalizer rebuilds `Final Answer`, `Source Audit`, `Claim Log`, `Limits/Conflicts/Research Debt`, and `Quality Gate` from persisted artifacts plus source diagnostics
- hidden machine-readable JSON remains appended after the visible appendix and does not count as visible compliance by itself
- narrative enrichment reads persisted artifacts and writes back only sanitized enriched artifacts, warnings, and specific research debt; raw enrichment prompts are transient and raw model output, provider payloads, source diagnostics, and controller artifact dumps remain local-only

Backward compatibility:

- event-only JSON from the earlier controller remains readable because missing fields default empty
- older artifacts without `narrative_state` deserialize as `None`
- older artifacts without `reader_quality` deserialize as `None`
- backward compatibility stays additive, but strict/high historical runs are expected to populate at least one of these hidden planning artifacts with useful depth instead of leaving both empty

## `narrative_state`

Additive schema stored inside `research_controller_artifacts_json`:

```json
{
  "version": 1,
  "topic_frame": "optional framing",
  "working_thesis": "optional thesis",
  "reader_promise": "optional reader promise",
  "timeline": [],
  "actors": [],
  "causal_chain": [],
  "evidence_layers": [],
  "interpretive_tensions": [],
  "impacts": [],
  "reader_questions": [],
  "section_outline": [],
  "transition_plan": [],
  "open_gaps": [],
  "last_iteration_summary": "optional"
}
```

Field roles:

- `timeline`: chronology scaffolding with stable event IDs and optional expected claim/source references
- `event_cards`: phased historical/process scaffold rows with `label`, `timeframe`, `actors`, `region_or_front`, `trigger`, `development`, `outcome`, optional `claim_log_ids`, optional `source_ids`, optional per-card `causal_spine`, optional per-card `interpretive_layers`, `confidence`, and `open_questions`
  - In strict/high historical runs, weak cards may be passed through the bounded historical event-card enrichment strategy. The strategy may fill missing phase fields and nested causal/interpretive detail, but only when the proposed fields cite existing supported Claim Log IDs and existing public Source Card IDs. Invented IDs, private/local source refs, or ungrounded details are dropped and recorded as specific research debt.
- `actors`: institutions, people, or groups the explanation should keep visible
- `causal_chain`: outline-only cause/effect sequence for the reader path
- `evidence_layers`: plan for presenting support from source-backed facts to interpretation and limits; not support itself
- `interpretive_tensions`: competing readings or contested explanatory frames that must be resolved by evidence or left open
- `impacts`: consequences or downstream implications the answer should cover when supported
- `reader_questions`: likely follow-up questions the answer should resolve or explicitly leave open
- `section_outline`: intended section progression
- `transition_plan`: bridge text plan between sections
- `open_gaps`: unresolved chronology, actor, causal, evidence-layer, impact, reader-question, or transition gaps
- `last_iteration_summary`: terse continuity note for the next loop iteration

Compatibility notes:

- `open_gaps` is canonical
- legacy `unresolved_structure_gaps` may be accepted as a deserialize alias and normalized into `open_gaps`

Evidence boundary:

- Narrative State is not reader-facing output
- Narrative State does not satisfy URL counts, authoritative source counts, supported-claim requirements, or conflict/debt resolution
- for strict/high historical runs, Narrative State is expected to carry useful chronology or phase structure plus supporting planning detail such as evidence layers, interpretive tensions, impacts, reader questions, or open gaps; broad historical process reports additionally require several event cards to act as phase dossiers rather than flat summaries
- for Hannibal / Second Punic War-class strict/high runs, grounded `event_cards` may also be used during finalization to restore visible campaign phase subsections, date anchors, and actor/front continuity when the draft collapses into a flat summary; this repair is structural only and does not weaken Source Card / Claim Log evidence gates
- `evidence_layers`, `interpretive_tensions`, `impacts`, `reader_questions`, and `open_gaps` may guide prompts and finalization order only when Source Cards and Claim Log support the resulting prose
- strict/high gates and finalizer-visible event-card prose require direct supported `claim_log_ids`; `source_ids` stay supplemental context and Source Card ID overlap alone is not enough
- `causal_spine` items use language-neutral `step_type` values such as `precondition`, `forcing_factor`, `decision_point`, `execution`, `contingent_moment`, `outcome`, and `forward_pressure`; each item carries its own `description`, `epistemic_status`, `reasoning`, `limits`, `claim_log_ids`, and `source_ids`
- `interpretive_layers` items use `layer_type` values such as `diplomacy`, `operations`, `logistics_economics`, `geography_front`, `domestic_politics`, and `historiography_limits`; each item carries its own `interpretation`, `epistemic_status`, `reasoning`, `limits`, `claim_log_ids`, and `source_ids`
- `epistemic_status` distinguishes `fact`, `interpretation`, `inference`, `hypothesis`, `contested`, and `limit`. Claim refs are evidence anchors, not truth guarantees: interpretations must say how the cited facts are being read, and inference may go one step beyond only when the reasoning chain is explicit and non-contradictory. Hypotheses/limits may appear as caveats but should not carry the main conclusion.
- strict broad-history validation counts grounded causal spine steps and grounded interpretive layers per card; many one-sentence claim-linked cards should fail rather than pass as narrative depth
- transient repair search hints are prompt-only leads; their URLs do not count as adopted evidence until normal adoption or independent fetch
- validation may reject reader-facing output that echoes repair-hint labels or uses hint provenance as if it were final evidence

Final-output hygiene:

- committed benchmark `*-final-output.md` files are sanitized reader-facing artifacts
- `controller-artifacts.json` retains `narrative_state` and the machine-readable continuity details used by prompts and finalization

## `reader_quality`

Additive schema stored inside `research_controller_artifacts_json`:

```json
{
  "argument_graph": {
    "nodes": [],
    "edges": []
  },
  "narrative_plan": {
    "lead_section_id": "optional",
    "section_ids": [],
    "transition_ids": [],
    "narrative_arc": "optional",
    "ending_note": "optional"
  },
  "section_briefs": [],
  "reader_critique": {
    "summary": "optional",
    "strengths": [],
    "weaknesses": [],
    "improvement_priorities": [],
    "metrics": []
  }
}
```

Field roles:

- `argument_graph.nodes`: compact reader-facing claim clusters or supporting lines that point back to `claim_log_ids` and `source_card_ids`
- `argument_graph.edges`: compact relationships between argument nodes such as `supports`, `qualifies`, or `contrasts`
- `narrative_plan`: lightweight ordering metadata that references existing `section_outline` / `transition_plan` IDs rather than duplicating prose
- `section_briefs`: per-section reader goal plus a compact key point, again anchored to claim/source IDs
- `reader_critique`: sanitized reader-facing critique only; use short strengths, weaknesses, improvement priorities, and compact metric rows

Compatibility and hygiene notes:

- `reader_quality` is optional and defaults to `None` for older artifacts and replay bundles
- strict/high historical runs are expected to use `reader_quality` when it is the main hidden planning scaffold; empty `reader_quality` is fine only if `narrative_state` already carries useful historical planning depth
- `reader_quality` is part of the same single hidden artifact envelope; there is no second hidden block
- compaction clears or truncates `section_briefs`, then `argument_graph` / `reader_critique`, before sacrificing evidence ledgers
- prompt-like content, raw diagnostics, provider payloads, and controller/prompt JSON are rejected or stripped during normalization
- committed benchmark `*-final-output.md` files do not keep `reader_quality`; the hidden artifact block is stripped before promotion
- open historical debt carried in the same envelope must not use generic placeholders such as `missing evidence not specified`; name the exact missing phase, actor, place/front, transition, source layer, or interpretive gap

## `research_source_diagnostics_json`

Envelope:

```json
{
  "version": 1,
  "subject": "optional topic",
  "source_pack": {},
  "scrapes": [],
  "context_packing": {}
}
```

Fields:

- `source_pack`: subject, status, reason, query reports, adopted/skipped candidates, adopted count, `coverage_misses`
- `scrapes`: original URL, normalized URL, final URL, status class, failure reason, extraction strategy, content type, size counts, sufficiency result, references, accessed-at, raw capture policy
- `context_packing`: strategy, included/omitted card counts, excerpt chars, omitted raw chars, active debt count, unresolved conflict count, additive narrative presence/count fields, omitted narrative chars, notes

Context-packing additive narrative fields:

- `narrative_state_present`
- `narrative_timeline_event_count`
- `narrative_section_count`
- `narrative_evidence_layer_count`
- `narrative_interpretive_tension_count`
- `narrative_impact_count`
- `narrative_reader_question_count`
- `narrative_open_gap_count`
- `narrative_omitted_chars`
- `reader_quality_present`
- `reader_argument_node_count`
- `reader_argument_edge_count`
- `reader_narrative_plan_present`
- `reader_section_brief_count`
- `reader_critique_present`
- `reader_critique_metric_count`
- `reader_critique_failed_metric_count`
- `reader_quality_omitted_chars`

Prompt and finalization behavior:

- persisted Narrative State may be rendered into controller and repair prompts as bounded XML-like outline blocks
- persisted Narrative State may be rendered into research context packs before evidence ledgers
- finalization may use supported Narrative State continuity to repair chronology, actor flow, transitions, and visible limits
- none of the above changes make Narrative State reader-facing evidence

`coverage_misses` rows are additive diagnostics used for replay/live scorecards and visible debt summaries:

- `query`: user-visible attempted query text
- `provider`: provider name when known
- `status`: `missed`, `blocked`, `found_not_adopted`, or `adopted`
- `expected_host`: derived target host when known
- `expected_source_class`: derived source class when known
- `reason`: redacted explanation only; never include provider credentials, raw headers, or raw provider payloads

Visible appendix note:

- the visible `Target Host / Source Class Misses` section renders only rows whose `status != adopted`, so adopted coverage expectations stay in machine-readable diagnostics without being shown as active misses

Structured replay note:

- replay-mode run local `.json/.csv/.ndjson` outputs include additive `replay_before` fields so graphable artifacts can show before/after trust recovery without reading markdown
- this sanitized snapshot commits reviewed `.md`, `.csv`, and sanitized `*-final-output.md` artifacts only

Raw capture policy:

- default mode is `omitted`
- blocked/private/local targets record an omission reason instead of a response body
