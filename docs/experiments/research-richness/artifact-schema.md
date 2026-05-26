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
  "quality_gate": {},
  "warnings": []
}
```

Fields:

- `events`: controller stage history preserved across retries
- `source_cards`: `id`, `url`, `title`, `source_class`, `accessed_at`, `extracted_facts`, `limitation`, `diagnostics_ref`, `confidence`
- `claim_log`: `id`, `claim`, `claim_type`, `support_source_card_ids`, `support_urls`, `confidence`, `uncertainty_note`, `needs_verification`
- `conflict_map`: `id`, `topic`, `conflicting_claim_ids`, `source_card_ids`, `resolution_status`, `resolution_note`, `promoted_to_debt`
- `research_debt`: `id`, `severity`, `failed_gate`, `missing_evidence`, `required_source_class`, `candidate_queries`, `next_check_actions`, `status`
- `narrative_state`: optional outline continuity artifact only; never evidence
- `quality_gate`: `status`, `failure_messages`, `unsupported_claim_count`, `unresolved_conflict_count`, `open_debt_count`
- `warnings`: parser, compatibility, or finalization warnings persisted after deterministic repair

Finalization notes:

- trusted save uses finalized visible output, not the raw model draft
- the finalizer rebuilds `Final Answer`, `Source Audit`, `Claim Log`, `Limits/Conflicts/Research Debt`, and `Quality Gate` from persisted artifacts plus source diagnostics
- hidden machine-readable JSON remains appended after the visible appendix and does not count as visible compliance by itself

Backward compatibility:

- event-only JSON from the earlier controller remains readable because missing fields default empty
- older artifacts without `narrative_state` deserialize as `None`

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
- `evidence_layers`, `interpretive_tensions`, `impacts`, `reader_questions`, and `open_gaps` may guide prompts and finalization order only when Source Cards and Claim Log support the resulting prose
- reader-facing narrative labels require supported Claim Log refs; Source Card ID existence alone is not enough
- transient repair search hints are prompt-only leads; their URLs do not count as adopted evidence until normal adoption or independent fetch
- validation may reject reader-facing output that echoes repair-hint labels or uses hint provenance as if it were final evidence

Final-output hygiene:

- committed benchmark `*-final-output.md` files are sanitized reader-facing artifacts
- `controller-artifacts.json` retains `narrative_state` and the machine-readable continuity details used by prompts and finalization

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
