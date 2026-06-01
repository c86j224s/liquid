# Liquid protocol crate audit

작성일: 2026-05-31
브랜치: `refactor/modular-workspace-roadmap`

## 결론

기존 `liquid-contracts`의 실제 내용은 대부분 contract/business invariant가 아니라 **Liquid 구성요소 사이에서 직렬화되어 오가는 protocol shape**다. 따라서 크레이트의 올바른 이름은 `liquid-protocol`이다.

이 감사의 기준은 다음과 같다.

- **Protocol에 남김**: DTO type, serde field/default/alias, artifact/replay/benchmark wire shape, protocol warning code, wire-level value grammar.
- **Natural owner로 이동**: 상태 전이, domain predicate, source authority 판단, research quality gate, provider/runtime policy, public redaction policy.
- **App에 남김**: DB/AppState/HTTP/provider/config/security publication policy에 의존하는 검증과 orchestration.

## 현재 symbol 분류

| 영역 | 대표 symbol | 판정 | 이유 |
| --- | --- | --- | --- |
| Wire value grammar | `VALID_FILE_STATUSES`, `VALID_SCRAPE_MODES`, `VALID_RESEARCH_INTENSITIES`, `VALID_RESEARCH_QUALITY_DEPTHS`, `is_valid_*` | STAY | serialized request/artifact 값의 protocol grammar다. 행동 의미는 각 domain owner가 해석한다. |
| Local source-pack artifact warning/detail tokens | `PI_LOCAL_SOURCE_PACK_*` | MOVED | protocol wire grammar가 아니라 acquisition/research repair artifact vocabulary이므로 `crates/liquid-research-artifacts`로 이동했다. acquisition/research-core/application은 같은 crate를 참조해 문자열 drift를 막는다. |
| Research controller artifacts | `ResearchControllerArtifacts`, `ResearchSourceCard`, `ResearchClaimLogEntry`, `ResearchConflictMapEntry`, `ResearchDebtItem`, `ResearchQualityGateArtifact` | STAY | artifact JSON shape와 serde compatibility다. quality 판단/repair/finalization은 `liquid-research-core`가 소유한다. |
| Reader/narrative artifacts | `Reader*`, `Narrative*` DTOs | STAY | narrative pipeline artifact wire shape다. narrative enrichment/merge/gate behavior는 research core/application 소유다. |
| Source diagnostics DTOs | `ResearchSource*Report`, `ScrapeDiagnostics`, `ResearchContextPackingDiagnostics` | STAY | source diagnostics payload shape다. search/fetch/acquisition policy는 `liquid-acquisition`/application 소유다. |
| Public summary DTOs | `ResearchControllerArtifactsSummary`, `ResearchScrapeDiagnosticsSummary`, `ResearchSourceDiagnosticsSummary` | STAY | public API summary wire shape다. redaction/publication policy는 `src/server/files/research_public_summary.rs` 소유다. |
| Benchmark/replay DTOs | `ResearchBenchmarkMode`, `ResearchBenchmarkCaseInput`, `ResearchBenchmarkCaseResult`, `ResearchReplayCaseInput` | STAY | benchmark CLI/replay boundary protocol이다. scoring/gate behavior는 `liquid-bench`/research core 소유다. |
| Serde compatibility helpers | `default_reader_critique_metric_status`, `deserialize_reader_critique_metric_status`, alias/default tests | STAY | backward-compatible protocol decoding이다. |

## 감사 결과

이번 감사에서 `liquid-protocol` 내부에 즉시 이동해야 할 명백한 domain behavior는 발견하지 않았다. 다만 `src/contracts.rs`의 scrape 실패 안내 문구는 DTO/protocol이 아니라 presentation policy이므로 `src/application/scraping.rs`로 이동했다. 다만 이름이 `contract`인 동안 `is_valid_*` 같은 함수가 business validation처럼 오해될 수 있으므로, 이름을 `liquid-protocol`로 바꿔 **wire grammar predicate**로 해석되게 한다.

프로토콜 크레이트에는 앞으로 다음을 넣지 않는다.

- task lifecycle legality
- domain predicate/state transition
- research quality/richness gate
- source authority decision
- provider capability/runtime policy
- public artifact redaction policy
- DB/AppState/config/HTTP status mapping

## Rename 원칙

`liquid-contracts` → `liquid-protocol` rename은 동작 변경이 아니라 naming correction이다. 타입명/field명/function명은 이번 PR에서 바꾸지 않는다. Cargo package name, dependency key, Rust crate path, 문서 참조만 기계적으로 변경한다.

## Research artifact vocabulary crate scope

`crates/liquid-research-artifacts` owns only stable string tokens and identifiers that are written into or recognized from generated research artifacts across multiple sibling crates. It must remain dependency-free unless a future token family truly requires protocol DTO names; it must not contain validation logic, prompt assembly, source acquisition behavior, finalization behavior, or workflow policy.

Current moved family:

- `PI_LOCAL_SOURCE_PACK_*`: Pi/local source-pack scaffold and repair warning/detail tokens. These are consumed by `liquid-acquisition`, `liquid-research-core`, and the root app repair/finalization flow, so neither acquisition nor research-core is the sole natural owner.

Sibling-token audit for this wave:

- `RESEARCH_CONTEXT_PACK_EXCERPTS_MARKER` stays in `liquid-acquisition`: it is a local context-pack rendering marker, not a cross-crate persisted artifact vocabulary family.
- `SOURCE_AUDIT_MARKERS`, `SOURCE_CARD_MARKERS`, `CLAIM_LOG_MARKERS`, `FINAL_ANSWER_MARKERS`, `QUALITY_GATE_MARKERS`, and repair/event leak markers stay in `liquid-research-core`: they are parser/finalizer heuristics owned by research-quality behavior, not shared artifact tokens.
- Benchmark marker lists stay in `liquid-bench`: they are scoring heuristics for benchmark reports, not product artifact vocabulary.
- `RESEARCH_CONTROLLER_ARTIFACT_VERSION`, stage constants, and source diagnostics version constants stay in the root application workflow: they describe DB/controller workflow compatibility, not reusable artifact vocabulary.

Change rule: additions are allowed when a token family is stable, cross-crate, and has no single natural producer/consumer owner. Renames/removals require major review because existing artifacts and DB records may already contain the old token values.
