# Liquid 모듈화 리팩토링 진행 기록

- 작성일: 2026-05-30
- 브랜치: `refactor/modular-workspace-roadmap`
- 목표: 서버, 저장소, 런타임, 스크래핑/소스 획득, 리서치 품질 로직을 단일 책임 경계로 나누고 이후 교체 가능한 crate seam을 확보한다.
- 현재 아키텍처 지도: `docs/architecture/intuitive-architecture.md`를 source of truth로 삼아 Core / Workflow / Adapters / Shell 네 레이어와 freeze-then-move / move-pure / move-with-trait 세 패턴만 사용한다.

## Wave 상태 요약

| Wave | 도메인 | 상태 | 비고 |
| --- | --- | --- | --- |
| Wave 1 | Server Persistence Shells | 완료 | route/API/SQLite 계약 유지, persistence helper만 storage repository 뒤로 이동 |
| Wave 2 | AI Runtime Provider And Prompt Boundary | 완료 | prompt/redaction/context guard 고정, provider execution body는 contract-guarded application adapter로 고정 |
| Wave 3 | Task Lifecycle And Research Orchestration | 완료 | `src/application/tasks.rs`를 private module shell들로 분해, event/write order 유지 |
| Wave 4 | Benchmark Public/Private Artifact Boundary | 완료 | public report renderer와 private/raw artifact I/O를 binary-local module로 분리 |
| Wave 5 | Cross-Domain Integration And Review Freeze | 완료 | 새 extraction 없이 docs/verification/artifact hygiene closeout |

## Server Shell Extraction Freeze

- 작성일: 2026-06-01
- 새 crate: `crates/liquid-server`
- 현재 ownership:
  - production HTTP shell, `ServerContext` service ports, presenters, and public research summary sanitizers move into `crates/liquid-server`
  - root `src/server/*.rs` stays as compatibility shim/re-export for old imports and app-side tests
  - `src/application/server_adapters.rs` and `src/main.rs` keep root composition over `AppState`, DB, provider execution, task lifecycle, and research implementation selection
- server crate constraints:
  - no direct `AppState` ownership
  - no direct provider/process execution ownership
  - no task lifecycle persistence ownership
  - no root composition responsibility beyond the `ServerContext` port surface
- moved into `crates/liquid-server`:
  - router/app bootstrap
  - server-facing service ports and `ServerContext`
  - task/file/presenter/sanitizer HTTP shell and public summary policy
  - research submission, scraping, engine-preset, drawer, and translate route shells
- remains in root:
  - composition/adapters over `AppState`
  - workflow implementations
  - DTO facades / compatibility shims
  - DB/task/provider execution and publication plumbing

Validation closeout log:

```text
cargo check --workspace --all-targets
cargo test -p liquid-server -- --nocapture
cargo test --lib -- --nocapture
cargo test -q --workspace
```

The commands above are recorded as a closeout log for the extraction, not as live quality proof.

## Files/Workspace Boundary Extraction

- 작성일: 2026-05-31
- 상태: 완료
- 새 crate:
  - `crates/liquid-files`
  - `crates/liquid-workspace`
- 최종 의존 방향:
  - `crates/liquid-server/src/files.rs` route shell; `src/server/files.rs` compatibility shim
  - `crates/liquid-workspace` file/task/document-link use case
  - `crates/liquid-files` file/content primitives + tag/link ports
  - `crates/liquid-storage-sqlite` adapter implementation
- `liquid-files` 소유:
  - `FileMetadata`, `FileListItem`
  - `TagInfo`
  - `ResearchSourceDocument`
  - document relationship graph primitives
  - `normalize_tag_slug`, `strip_legacy_title_metadata`, markdown preview helper, system tag policy
  - file/tag/document-link port traits
- `liquid-workspace` 소유:
  - research-request assembly
  - source document hydration
  - task-output provenance / document-link orchestration
  - research-request task/context port vocabulary
- `liquid-storage-sqlite` 역할:
  - schema / migrations / SQL ordering 유지
  - `liquid-files` / `liquid-workspace` port 구현
  - route/task shell이 보던 file/tag/link read path를 domain structs로 직접 반환
- server shell에 남긴 것:
  - Axum handlers
  - status code / response mapping
  - uploads path / filesystem read-write
  - public research summary sanitizer
  - user-tag validation and route-level request checks
- 의도적으로 이동하지 않은 것:
  - `AppState`
  - scrape ownership
  - translation ownership
  - concrete research implementation
  - public artifact trust boundary

## Research Implementation Extraction Freeze

- 작성일: 2026-05-31
- 대상 boundary: `crates/liquid-research-classic`
- 선택 이유:
  - 현재 리서치 구현을 app workflow/shell에서 분리하되, scraping/translation/provider/DB/task/public-artifact trust boundary는 유지해야 한다.
  - `liquid-research-core`는 순수 validator/finalizer/context primitive로 남기고, 기존 "classic" 전략 계층은 별도 crate로 두는 편이 replaceability를 더 명확하게 만든다.
- 현재 stage 비범위:
  - multi-turn research, session state, conversation storage, runtime UI switching, provider execution migration, scraping ownership 이동, translation ownership 이동

### Execution Block 1 Status

- 상태: 완료
- 추가 경계:
  - `crates/liquid-research-classic`를 생성하고 classic prompt/template/context-pack policy와 pure artifact merge/debt/narrative preservation, repair prompt shaping, local-pi scaffold/provenance policy를 이동했다.
  - startup config는 `LIQUID_RESEARCH_IMPLEMENTATION=classic` 한 값만 허용하며, `AppState`는 selected implementation ID와 app-facing `ResearchImplementation` port를 가진다.
  - `crates/liquid-server/src/research.rs`와 `src/application/ai_runtime.rs`, `src/application/tasks/{queue_workflow,execution_shell,benchmark_runtime}.rs`는 research prompt/context 선택을 `AppState.research_implementation`으로 위임하고 `src/server/research.rs`는 compatibility shim으로 남긴다.
- 이번 block에서 의도적으로 유지한 경계:
  - scraping/source acquisition
  - translation execution
  - provider execution / timeout / stderr / stdout handling
  - DB/task lifecycle/status/event ordering
- public artifact trust / sanitized publication boundary

### Execution Block 3 Status

- 상태: 완료
- 추가 경계:
  - `src/application/tasks/narrative_merge.rs`의 순수 artifact merge / debt / event-card / narrative preservation policy를 `crates/liquid-research-classic`로 이동했다.
  - `src/application/tasks/repair_helpers.rs`의 순수 repair prompt shaping / debt wording policy를 `crates/liquid-research-classic`로 이동했다.
  - `src/application/tasks/execution_shell.rs`의 local-pi scaffold / provenance pure helper를 `crates/liquid-research-classic`로 이동했다.
  - `src/application/ai_runtime/research_context_workflow.rs`의 no-row fallback도 직접 classic helper가 아니라 `state.research_implementation` seam을 사용하도록 고정했다.
- 이번 block에서 의도적으로 유지한 경계:
  - task claim / queue / lifecycle status policy
  - DB writes / artifact persistence / validation persistence
  - provider execution / scraping / translation
  - finalization / validation orchestration
  - public artifact sanitization / publication trust boundary

### Wave 1 Symbol Classification

| 위치 | 심볼 | 분류 | 메모 |
| --- | --- | --- | --- |
| `src/application/research_prompts.rs` | `research_allows_web_search`, `web_search_provider_for`, `web_search_audit_prompt`, `intensity_prompt`, `research_controller_contract_prompt`, `fallback_disclosure_prompt`, `normalize_research_mode`, `normalize_research_type`, `research_mode_lens`, `build_research_system_prompt`, `research_focus_section`, `build_document_research_user_prompt`, `build_follow_up_research_user_prompt`, `build_topic_research_user_prompt` | `MOVE_TO_CLASSIC` | classic prompt policy. HTML design prompt helper도 같은 전략 계층으로 이동 가능 |
| `src/application/ai_runtime/research_context_workflow.rs` | `PreparedAiContext`, `prepare_ai_context`, `load_raw_ai_context` | `STAY_APP_SHELL` | `AppState`, filesystem, task-row artifact load에 의존 |
| `src/application/ai_runtime/research_context_workflow.rs` | `build_research_context_pack`, `raw_fallback_context_diagnostics` | `MOVE_TO_CLASSIC` | provided artifacts/documents 위의 pure context planning wrapper |
| `src/application/tasks/narrative_merge.rs` | `research_quality_gate_from_failures`, `conflict_status_is_terminally_resolved`, `merge_research_controller_artifacts`, `merge_research_controller_artifacts_with_trusted_source_urls`, `trusted_artifact_merge_source_urls`, `strip_untrusted_planning_evidence_refs`, `reconcile_research_debt_snapshot`, `merge_reader_quality`, `merge_narrative_state`, `enrich_narrative_state_from_event_cards`, `upsert_derived_narrative_debt`, `resync_derived_narrative_debt`, `narrative_card_label`, `narrative_card_trigger_or_label`, `narrative_card_outcome_or_label`, `compact_narrative_text`, `should_preserve_existing_event_cards`, `narrative_event_card_claim_reference_count`, `narrative_event_card_reference_score`, `merge_event_cards_preserving_existing_scope`, `event_cards_represent_same_scope`, `event_card_phase_terms`, `event_card_key_contains`, `normalize_event_card_key`, `narrative_event_card_richness_score`, `merge_narrative_open_gaps`, `narrative_gap_is_closed`, `narrative_gap_converted_to_debt`, `upsert_research_debt`, `push_unique_warning`, `close_research_debts_for_gate`, `sync_research_debts_for_gate`, `normalize_deferred_conflicts_to_actionable_debt`, `conflict_needs_deterministic_debt_promotion`, `build_actionable_conflict_debt`, `merge_actionable_conflict_debt`, `merge_conflict_debt_list`, `conflict_debt_id`, `conflict_candidate_queries`, `conflict_next_check_actions`, `join_conflict_refs` | `MOVE_TO_CLASSIC` | pure artifact merge/debt/narrative policy |
| `src/application/tasks/repair_helpers.rs` | `build_quality_repair_prompt`, `normalize_absolute_public_support_url`, `render_repair_claim_context_block`, `render_repair_search_hints_block`, `final_answer_depth_repair_guidance`, `historical_development_repair_guidance`, `historical_event_card_repair_guidance`, `historical_narrative_artifact_repair_guidance`, `historical_narrative_artifact_repair_should_trigger`, `historical_event_card_repair_should_trigger`, `historical_event_card_prompt_wording`, `technology_repair_guidance`, `technology_like_repair_subject`, `technology_concept_like_repair_subject`, `attention_like_concept_subject`, `policy_or_regulatory_like_repair_subject`, `technology_implementation_like_repair_subject`, `technology_repair_marker_present`, `contains_ascii_repair_token_with_boundaries`, `conflict_debt_repair_guidance`, `debt_id_from_failure`, `required_source_class_from_failure`, `candidate_queries_from_failure` | `MOVE_TO_CLASSIC` | pure repair prompt shaping and debt wording |
| `src/application/tasks/repair_helpers.rs` | `claim_next_ai_task`, `claimable_task_sql`, `target_status_for_prefix`, `normalized_quality_max_iterations`, `research_controller_max_iterations`, `normalized_quality_depth` | `STAY_APP_SHELL` | queue/status/task lifecycle policy |
| `src/application/tasks/repair_helpers.rs` | `collect_repair_search_queries`, `push_repair_search_query`, `derive_coverage_miss_repair_query`, `sanitize_repair_search_query`, `normalize_repair_search_query_key`, `collect_repair_search_known_urls`, `refresh_pending_repair_hint_urls`, `repair_search_subject`, `compact_repair_topic_query` | `STAY_ACQUISITION` | repair search hint acquisition/support path와 묶어 유지 |
| `src/application/tasks/execution_shell.rs` | `finalize_task_research_output`, `validate_task_research_output`, `research_source_diagnostics_subject` | `STAY_APP_SHELL` + classic pure helper extraction | DB/task row load/persist는 app shell에 남기고, finalization/validation strategy body는 classic helper로 분리 가능 |

### Classic Boundary API Freeze

현재 classic boundary가 실제로 소유하는 책임:

- initial/topic/follow-up research prompt policy 생성
- research mode/type normalization
- provided source documents/artifacts/diagnostics 위의 context-pack planning
- artifact merge / debt / narrative preservation policy
- repair prompt shaping
- local-pi scaffold / provenance pure policy

계획상 다음 단계에서만 검토할 책임(현재 classic boundary 비소유):

- scraping/source discovery/repair hint fetch
- translation workflow 또는 `[KO]` execution
- provider execution / timeout / stderr / stdout handling
- resolved prompt persistence redaction
- DB writes, task lifecycle/status/event ordering
- finalization / validation orchestration
- HTTP request/response shell
- public artifact publication / sanitized public summaries

### Research Implementation Extraction Closeout

- 상태: 완료
- 새 crate: `crates/liquid-research-classic`
- app-facing seam: `src/application/ports.rs`의 `ResearchImplementation`
- startup selection: `LIQUID_RESEARCH_IMPLEMENTATION=classic`만 현재 허용한다. 다른 구현은 같은 trait/설정 지점에 새 implementation을 붙이는 방식으로 확장한다.
- classic crate가 소유하는 구현:
  - research prompt/template policy
  - research mode/type normalization
  - web-search audit wording
  - pure context-pack planning wrapper
  - artifact merge / debt / narrative preservation policy
  - repair prompt shaping
  - local-pi scaffold / provenance pure policy
- Liquid app에 남긴 책임:
  - scraping/source acquisition 실행과 repair search hint fetch
  - translation workflow
  - provider process/HTTP execution, timeout, stdout/stderr handling
  - DB writes, task lifecycle/status/event ordering, cancellation
  - finalization/validation orchestration
  - HTTP route/response shell
  - public artifact publication and redaction boundary
- boundary hardening:
  - `liquid-research-classic`는 `liquid-acquisition`에 직접 의존하지 않는다.
  - evidence URL trust policy는 `liquid-research-core`의 shared absolute public URL helper로 모았다. core artifact validation, classic artifact merge, scaffold support/card construction, repair claim-context support rendering이 같은 helper를 사용한다.
  - persisted artifact trust path는 relative URL이나 DuckDuckGo relative `uddg` redirect를 public evidence처럼 승격하지 않고 absolute public HTTP(S) URL만 신뢰한다. URL parsing은 대소문자 HTTP(S) scheme을 정상 허용하되 decoded redirect target도 absolute public HTTP(S)여야 한다.
  - repair prompt diagnostics는 raw scrape/source-pack failure text를 직접 반영하지 않고 allowlisted status/category만 노출한다. 안전한 `skipped` source-pack status는 보존하고 unknown status는 `unknown`으로 축약한다.
  - scaffold source-quality scoring은 acquisition 시절의 scholarly/reference high-confidence host와 historical contested-source downgrade 동작을 classic-local pure policy로 보존한다.

## HTML Report Skill Boundary

- 상태: 완료
- 새 crate: `crates/liquid-html-report`
- 선택 이유:
  - HTML report design prompt는 classic research 구현 전용이 아니라, 다른 research implementation이나 향후 non-research HTML report에도 재사용될 수 있다.
  - 단일 `SKILL.md` 파일뿐 아니라 작은 local skill directory가 `SKILL.md`와 bounded reference material을 가질 수 있으므로, app shell이 파일 내용을 직접 읽어 문자열로 넘기면 HTML report skill 해석 책임이 앱에 새어 들어간다.
- 경계:
  - Liquid app은 `LIQUID_RESEARCH_HTML_SKILL_PATH` 같은 env/config에서 “어떤 path를 쓸지”만 고른다.
  - `liquid-html-report`는 caller-provided path를 HTML report skill source로 해석한다. 현재 지원 shape는 direct `SKILL.md`/`.md`/`.txt` file 또는 directory `SKILL.md`이며, reference support는 allowlisted `references/component-patterns.md`로 제한한다.
  - crate는 env var, AppState, DB, network, research task state를 읽지 않는다.
  - local skill source는 bounded size로 읽고 symlink source/reference는 reject한다. 실패 시 built-in HTML report prompt로 fail closed한다.
  - task DB와 resolved prompt storage에는 external skill 본문을 그대로 저장하지 않고 `[external-html-report-skill-redacted]` marker로 보존한다. 실행 시점에는 현재 configured path에서 다시 hydrate해 모델에는 design guidance를 전달하되, local skill 본문을 durable task metadata로 승격하지 않는다.
- 비범위:
  - manifest/registry/marketplace
  - network skill loading
  - asset/script execution
  - Markdown/PDF report policy

## 완료된 경계

| 경계 | 새 위치 | 루트 shim/잔여 역할 | 상태 |
| --- | --- | --- | --- |
| 파일/콘텐츠 primitives 및 tag/link ports | `crates/liquid-files` | `src/contracts.rs`는 root-local facade re-export만 유지 | 완료 |
| Server shell / presenters / sanitizers | `crates/liquid-server` | `src/server/*.rs` compatibility shim, `src/application/server_adapters.rs` composition bridge, and `src/main.rs` keep the root app wired; `ServerContext` service ports live in the server crate | 완료 |
| 파일/작업/문서 링크 workspace use case | `crates/liquid-workspace` | `crates/liquid-server/src/files.rs`와 `src/application/tasks/completion_shell.rs`는 shell/composition만 유지하고 `src/server/files.rs`는 compatibility shim이다 | 완료 |
| 리서치 품질/아티팩트 판정 | `crates/liquid-research-core` | legacy `src/research_quality.rs` shim 제거 | 완료 |
| SQLite 저장소 기반 | `crates/liquid-storage-sqlite` | legacy `src/db.rs` shim 제거, 일부 handler SQL은 서버 shell에 보존 | 완료 |
| 파일/태그/문서 링크 저장소 헬퍼 | `crates/liquid-storage-sqlite` | `crates/liquid-server/src/files.rs`는 HTTP handler와 thin adapter 중심이고 `src/server/files.rs`는 compatibility shim이다 | 완료 |
| 서버 persistence shell Wave 1 | `crates/liquid-storage-sqlite` + `crates/liquid-server` shell | `crates/liquid-server/src/engine_presets.rs`, `crates/liquid-server/src/files.rs`는 route/validation/response shell 보존; `src/server/*.rs`는 compatibility shim으로 남음 | 완료 |
| 소스 획득/리서치 소스 모델 | `crates/liquid-acquisition` | legacy `src/research_sources.rs` shim 제거 | 완료 |
| 순수 스크래핑 헬퍼 | `crates/liquid-acquisition` | `src/application/scraping.rs`는 guarded network fetch/helper를 보존하고 `crates/liquid-server/src/scraping.rs`는 HTTP validation/task enqueue/response shell을 맡으며 `src/server/scraping.rs`는 compatibility shim이다 | 완료 |
| CLI/Pi 런타임 | `crates/liquid-runtime` | legacy `src/cli_launcher.rs`, `src/pi_runtime.rs` shim 제거 | 완료 |
| Root contracts facade / public export gate | `src/contracts.rs` + `src/lib.rs` | `src/models.rs` shim 제거, root DTO inventory와 public re-export surface를 한곳에서 유지 | 완료 |
| 리서치 artifact vocabulary | `crates/liquid-research-artifacts` | Pi/local source-pack scaffold and repair warning/detail tokens를 protocol wire grammar에서 분리 | 완료 |
| 엔진 preset 순수 정책 | `crates/liquid-runtime::engine_presets` | `src/application/engine_presets/`는 catalog/SQL hydration/resolution/test-status implementation을 보존하고 runtime pure policy는 `crates/liquid-runtime::engine_presets`가 맡으며 `crates/liquid-server/src/engine_presets.rs`는 HTTP handlers와 response mapping을 맡고 `src/server/engine_presets.rs`는 compatibility shim이다 | 완료 |
| AI prompt/redaction 순수 런타임 헬퍼 | `crates/liquid-runtime::{ai_prompt_redaction, ai_translation}` | `src/application/ai_runtime.rs`와 `src/application/ai_runtime/provider_runtime.rs`가 provider 실행, DB 저장, task 상태 전이를 보존 | 완료 |
| AI provider/prompt boundary Wave 2 guard | `src/application/ai_runtime.rs` + focused contract tests | provider dispatch body는 application adapter에 보존, prompt/redaction/context snapshot과 pre-launch/provider-guard contracts만 고정 | 완료 |
| AI provider runtime submodule | `src/application/ai_runtime/provider_runtime.rs` | `src/application/ai_runtime.rs`는 provider dispatch decision을 보존하고 submodules가 prompt storage/context/translation/provider body를 나눈다 | 완료 |
| 리서치 context-pack 구성 | `crates/liquid-research-core::research_context` | `src/application/ai_runtime.rs`는 서버 상태에서 artifacts를 읽어 research-core helper에 전달 | 완료 |
| Task lifecycle/orchestration shell | `src/application/tasks/{queue_workflow,benchmark_runtime,execution_shell,narrative_merge,repair_helpers,completion_shell,benchmark_fixture,helpers,lifecycle_policy,retry_policy,runtime_adapters}` | `src/application/tasks.rs`는 module wiring, shared constants/types, tests 보존하고 adapters는 `src/application/tasks/runtime_adapters.rs`가 소유 | 완료 |
| Benchmark public/private artifact boundary | `crates/liquid-bench/src/{main,artifact_io,report_render}.rs` | workspace `default-members`가 `cargo run --bin research_bench` compatibility를 보존하고 root app crate는 binary ownership에서 분리 | 완료 |
| Research classic policy | `crates/liquid-research-classic` + `src/application/research_prompts.rs` compatibility shim | `crates/liquid-server/src/research.rs`는 HTTP submission handlers, DB/task creation, route-facing response shape를 보존하고 prompt/template, research mode/type normalization, web-search audit wording, context-pack planning, artifact merge/debt/narrative preservation, repair prompt shaping, scaffold/provenance policy는 selected `ResearchImplementation`으로 위임하며 `src/server/*.rs`는 compatibility shim으로 남는다 | 완료 |

## 의도적으로 남긴 경계

- `src/application/ai_runtime.rs` provider execution orchestration: provider 실행 본문은 `src/application/ai_runtime/provider_runtime.rs`로 adapter body 파일 경계를 만들었고 invocation/failure/transport contract로 고정했다. 별도 crate까지는 이동하지 않는다. resolved prompt 저장, task DB/status write order, cancellation/queue lifecycle, timeout/stderr formatting, raw-output sanitization 정책이 한 트랜잭션처럼 맞물려 있어 여기서 더 밀면 mechanical extraction이 아니라 behavior rewrite가 된다.
- `crates/liquid-server/src/research.rs` route-facing submission shell: prompt/template 본문은 `src/application/research_prompts.rs`로 분리했지만, HTTP handler, task metadata assembly, DB insert, response shape는 server crate shell에 남기고 `src/server/*.rs`는 compatibility shim으로 남긴다.
- `src/application/engine_presets/` implementation + `crates/liquid-server/src/engine_presets.rs` route wrapper: `/api/engine-presets` route, validation, status code, response shape, test endpoint shell은 server crate에 남겼고, custom preset persistence와 pure policy는 각각 storage/runtime 경계 뒤로 이동했고 app engine preset module은 catalog/resolver/status adapter로 남았다.
- `src/application/tasks.rs`는 shared adapter/test host 역할을 유지하고, `crates/liquid-server/src/files.rs`는 Axum handler와 HTTP response shell을 유지한다. 두 쪽의 큰 내부 흐름은 각각 private module/repository seam으로 분해했다.

이 예외들은 “아직 못 한 일”이라기보다 다음 단계가 adapter 설계·계약 테스트·정책 결정을 요구하는 경계다. 특히 provider body를 crate로 빼는 일은 provider stdout/stderr/timeout, raw prompt/payload redaction, DB write ordering의 실패 모드가 곧 보안/데이터 보존 문제로 이어지므로 별도 승인된 wave가 필요하다.

## 보안/신뢰 경계 메모

- Pi `web_fetch`는 모델/도구 입력을 받는 공격면이므로 public HTTP(S) URL, private/link-local/metadata IP, IPv4-mapped IPv6, redirect를 수동 검증한다.
- 스크래핑 helper 이동은 네트워크 fetch를 새 crate로 옮기지 않고 순수 문자열/HTML/Markdown 변환 중심으로 제한했다.
- Research benchmark raw outputs, provider payload, local `.env`, DB/log는 repo에 넣지 않았다.

## 검증 기록

Wave 5 통합 검증 / review freeze:

```text
cargo fmt --check
git diff --check
cargo check --workspace --all-targets
cargo test -q --workspace
cargo test -q public_reexports_match_contract_inventory --lib
cargo test -q route_inventory_snapshot_matches_current_router_surface --lib
cargo test -q prompt_text_and_redaction_snapshot_matrix_is_frozen --lib
cargo test -q task_contract_snapshot_matrix_is_frozen --lib
cargo test -q benchmark_cli_surface_matches_contract_inventory --bin research_bench
cargo test -q benchmark_cli_help_describes_headless_runner_contract --bin research_bench
cargo run --quiet --bin research_bench -- --label wave5-integration-freeze --runs-dir /tmp/liquid-wave5-integration.YqURNZ
```

Fixture smoke 결과:

```text
Cases: 7
Completed: 7
Quality passed: 6
Quality statuses: passed=6, untrusted=1
```

Historical fixture의 `untrusted=1`은 기존 연구 품질 게이트가 역사 narrative artifact/reader-facing richness를 엄격하게 유지하는 의도된 smoke 기준이며, 이번 모듈 추출의 회귀로 보지 않는다.

Wave 5 closeout 확인 사항:

- `src/lib.rs` public re-export 계약은 유지된다.
- `crates/liquid-server/src/app.rs` route inventory snapshot은 그대로 통과한다.
- prompt/redaction snapshot, task lifecycle snapshot, benchmark CLI compatibility tests는 모두 통과한다.
- public benchmark output은 `/tmp/liquid-wave5-integration.YqURNZ`에서 `.md/.json/.csv/.ndjson`와 reviewed `*-final-output.md`/per-case summary `.json`만 노출했고, report 본문은 `Resolved system prompt`, `Resolved user prompt`, `Source diagnostics JSON`, `Controller artifacts JSON`을 모두 `unavailable`로 표시한다.
- artifact hygiene scan에서 raw diagnostics, provider payload marker, `.env`, DB/log 경로 노출은 확인되지 않았다.



## Final Wave Unified Modular Boundary Closeout

2026-05-30 Final Wave에서는 Wave 5 이후 남은 “한 번 더 밀 수 있는” 경계를 하나로 묶어 닫았다. 기준은 “route/API와 raw diagnostic 보안 정책은 그대로 두고, root crate에 남아 있던 큰 파일 책임을 독립 파일 또는 별도 crate 경계로 이동한다”였다.

- `src/application/ai_runtime/provider_runtime.rs`: Ollama/CLI/Pi provider 실행 helper를 `ai_runtime` 내부 provider runtime module로 분리했다. `prompt_storage`, `research_context_workflow`, `translation_workflow`도 별도 submodule로 분리했다. `execute_ai_backend`, resolved prompt 저장, task DB/status lifecycle은 root orchestration에 남겨 provider migration이 DB write order나 raw error sanitization을 바꾸지 않게 했다.
- `src/application/research_prompts.rs`: research mode/type normalization, web search provider/audit prompt, document/follow-up/topic prompt builders, disclosure/intensity/focus prompt를 route handler에서 분리했다. `crates/liquid-server/src/research.rs`는 요청 검증, task 생성, metadata/response shell만 맡고 `src/server/*.rs`는 compatibility shim으로 유지된다.
- `crates/liquid-bench`: `research_bench` binary를 workspace member crate로 이동했다. `artifact_io`와 `report_render` private module 경계는 유지하고, workspace `default-members`로 기존 `cargo run --bin research_bench` 사용성을 보존했다.
- Public benchmark artifact hardening: provider failure text, replay-before failure text, URL-like tokens, bare private IPv4/IPv6/metadata hosts, JSON/colon/path/punctuation host fragments, prompt/debug markers, secret-looking markers, search-provider env labels를 public report/JSON/CSV/NDJSON에 쓰기 전에 sanitize/allowlist한다. raw provider/debug material은 여전히 explicit raw-debug opt-in 경계 밖으로 승격하지 않는다.
- Provider invocation contract freeze: Gemini/Claude/Codex/Pi invocation construction을 pure builder helper로 분리하고, provider별 executable/args/env/env_remove/web-search flag/sandbox-sensitive flag를 테스트로 고정했다. 이 단계는 provider body crate migration 자체가 아니라, migration 전에 drift를 잡기 위한 첫 Provider Boundary Contract Freeze다.
- Provider failure taxonomy freeze: provider failure branches를 `ProviderFailureKind`/`ProviderFailure`와 단일 `mark_task_failed` persistence helper로 이름 붙였다. 기존 `tasks.status='failed'`와 `error_message` 저장 shape는 유지하되, unknown engine, Pi setup/model/session/web-search, CLI build/spawn/wait/exit/timeout, Ollama HTTP/malformed response branch를 migration 전 contract surface로 드러냈다. Ollama client-build silent `None`은 현재 behavior로 남겨 두었고 다음 policy pass에서 별도 판단한다.
- Provider success/input transport freeze: Ollama request construction, Ollama response parsing, CLI success stdout extraction을 pure helper로 분리하고 tests로 고정했다. Ollama는 `stream=false`, `system=Some(...)`, `prompt=user_prompt` contract를 유지하고, CLI success output은 기존처럼 stdout을 lossy UTF-8로 변환하며 trailing newline을 보존한다.
- Storage/raw public summary policy freeze: `/api/files/:filename/research-request`가 raw controller artifact JSON과 source diagnostics JSON을 직접 노출하지 않고, `crates/liquid-server/src/files/research_public_summary.rs`의 public summary helper를 통해 counts/hosts/bounded diagnostics만 반환하도록 파일 경계를 만들고 tests로 고정했다. raw capture path/hash, resolved prompt-like event detail, source-card claim text, URL query/secret은 public response summary로 승격하지 않는 contract다.
- Task lifecycle/cancellation/queue policy freeze: `src/application/tasks/lifecycle_policy.rs`에 delete-cancellable status, abort-recovery status, queued-prefix target status, cancellation message를 이름 붙였고 `src/application/tasks/retry_policy.rs`가 retry eligibility/metadata reconstruction만 맡는다. 실제 queue worker/DB write/AbortHandle ownership은 application task shell에 남기되, queued delete와 running abort recovery가 서로 다른 status set을 가진다는 점, missing-model queued task가 failed event 후 같은 lane의 다음 작업으로 계속 진행한다는 점을 tests로 고정했다.

Final Wave 검증:

```text
cargo fmt
cargo check --workspace --all-targets
cargo test -q prompt_text_and_redaction_snapshot_matrix_is_frozen --lib
cargo test -q benchmark_cli_surface_matches_contract_inventory -p liquid-bench --bin research_bench
cargo test -q benchmark_cli_help_describes_headless_runner_contract -p liquid-bench --bin research_bench
cargo test -q -p liquid-bench --bin research_bench public_artifact_error_text_is_sanitized_before_rendering
cargo test -q -p liquid-bench --bin research_bench public_report_sanitizes_case_prompt_text
cargo test -q -p liquid-bench --bin research_bench configured_search_provider_labels_are_allowlisted_for_public_reports
cargo test -q -p liquid-bench --bin research_bench public_model_and_metadata_labels_redact_untrusted_values
cargo test -q -p liquid-bench --bin research_bench replay_fixture_prompt_stops_at_mixed_case_repair_marker
cargo test -q provider_invocation_spec_contract_is_frozen_for_cli_providers --lib
cargo test -q provider_invocation_spec_contract_is_frozen_for_pi --lib
cargo test -q provider_failure_taxonomy_messages_are_frozen --lib
cargo test -q provider_cli_exit_failure_uses_existing_stdout_stderr_precedence --lib
cargo test -q ollama_request_and_response_transport_contract_is_frozen --lib
cargo test -q provider_success_stdout_preserves_lossy_utf8_and_trailing_newlines --lib
cargo test -q public_research_summary_omits_raw_diagnostics_and_redacts_urls --lib
cargo test -q public_controller_artifact_summary_is_counts_only --lib
cargo test -q public_diagnostic_redaction_consumes_secret_values --lib
cargo test -q task_lifecycle_status_contract_is_frozen --lib
cargo test -q abort_recovery_marks_only_active_execution_statuses_interrupted --lib
cargo test -q delete_task_cancels_running_task_and_keeps_interrupted_row --lib
cargo test -q queue_claim_marks_missing_model_failed_and_continues_lane_scan --lib
cargo test -q --workspace
cargo run --quiet --bin research_bench -- --help
cargo run --quiet --bin research_bench -- --label final-unified-wave-final9 --runs-dir /tmp/liquid-final-unified-wave-final9-1780094410
```

Fixture smoke 결과는 기존 deterministic 기준과 동일하다.

```text
Cases: 7
Completed: 7
Quality passed: 6
Quality statuses: passed=6, untrusted=1
```

## Research Implementation Extraction 검증 기록

2026-05-31 `liquid-research-classic` extraction closeout:

```text
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test -q -p liquid-research-core
cargo test -q -p liquid-research-classic
cargo test -q --workspace
cargo tree -p liquid-research-classic | rg liquid-acquisition
git diff --check
cargo run --quiet --bin research_bench -- --mode fixture --label research-implementation-classic-final-closeout --runs-dir /tmp/liquid-research-classic-fixture-final-closeout-20260531T204710
```

결과:

```text
classic dependency guard: no liquid-acquisition dependency
core/classic URL trust regressions: passed
workspace tests: passed
fixture cases: 7
fixture completed: 7
fixture quality passed: 6
fixture quality statuses: passed=6, untrusted=1
```

`historical-explanation` fixture의 `untrusted=1`은 기존 deterministic fixture quality gate가 history narrative richness를 엄격히 표시하는 기준이며, 이번 crate extraction의 compile/runtime 회귀로 보지 않는다.

최종 판단: 목표로 삼은 coarse modular boundary는 닫았다. 추가로 남은 provider body crate migration은 source/prompt/payload redaction과 DB lifecycle을 함께 재설계해야 하므로 별도 adapter wave로만 진행한다.

이 문서의 검증 기록은 현재 상태를 재현하는 closeout 로그이며, live quality proof로 제시하지 않는다. fixture 결과의 `passed=6, untrusted=1`은 deterministic gate 상태를 보여주는 것이고 운영 품질 보증을 뜻하지 않는다.


## Wave 1 Server Persistence Shells 추가 기록

2026-05-29 Wave 1에서는 서버 persistence shell을 route/API 변경 없이 얇게 만들었다. 기준은 “HTTP handler는 검증·status code·response shape를 유지하고, 반복 SQL/persistence만 `liquid-storage-sqlite` repository method 뒤로 이동한다”였다.

- `liquid-storage-sqlite`에 추가: file CRUD-by-filename, status/title/delete lookup, tag listing/file-tag hydration, custom engine preset persistence methods.
- `crates/liquid-server/src/engine_presets.rs`: built-in preset order/negative IDs/default handling은 유지하고 custom preset SQL만 repository shell로 전환.
- `crates/liquid-server/src/files.rs`: file create/status/title/delete/lookups, tag list/load, document-link relationship loading 경로를 storage helper 뒤로 이동.
- `src/server/app.rs`, `src/application/ports.rs`: route inventory와 기존 seam이 충분해 변경하지 않았고 `src/server/app.rs`는 compatibility shim으로 유지된다.

Wave 1 검증:

```text
cargo fmt --check
git diff --check
cargo check --workspace --all-targets
cargo test -q --workspace
cargo test -q route_inventory_snapshot_matches_current_router_surface --lib
cargo test -q engine_preset_route_contract_snapshot_is_frozen --lib
cargo test -q update_tags_replaces_only_user_tags_and_preserves_system_tags --lib
cargo test -q test_status_archived_clears_drawer_and_invalid_status_rejected --lib
cargo test -q list_and_search_files_include_tag_metadata --lib
cargo test -q get_file_relationships_returns_sources_and_derivatives --lib
cargo test -q setup_db_backfills_document_links_from_completed_tasks_idempotently -p liquid-storage-sqlite
cargo run --quiet --bin research_bench -- --label wave1-persistence-fixture --runs-dir /tmp/liquid-wave1-persistence-fixture-1780062803
```

Fixture smoke 결과는 `Cases: 7`, `Completed: 7`, `Quality passed: 6`, `Quality statuses: passed=6, untrusted=1`로 기존 deterministic 기준과 동일하다.

## Wave 1 Runtime / Engine Boundary 추가 기록

2026-05-29 추가 wave에서는 `ai_runtime`에서 순수 문자열/컨텍스트 구성 책임만 먼저 분리했다. 경계 판단은 “서버 상태를 변경하거나 provider를 실행하는 코드는 루트에 남기고, 입력 artifact를 안전한 prompt/context 문자열로 바꾸는 순수 함수만 crate로 이동한다”는 기준을 따랐다.

- `liquid-runtime`으로 이동: resolved prompt 저장 전 redaction, 한국어 번역 prompt/chunk prompt 조립.
- `liquid-research-core`로 이동: research controller artifact와 source diagnostics를 untrusted/planning-only context pack으로 렌더링하는 구성 로직.
- 루트에 보존: provider dispatch, CLI/Ollama 실행, DB write order, task lifecycle, source-pack diagnostics 기록, HTTP/API surface.
- 중간 수정: 최초 구현안의 runtime → research-core 의존 방향은 재사용성/계층 경계에 맞지 않아 폐기하고, research 모델에 의존하는 context-pack 로직을 `liquid-research-core`로 재배치했다.

추가 검증:

```text
cargo check --workspace --all-targets
cargo test -q --workspace
cargo test -q engine_preset_route_contract_snapshot_is_frozen --lib
cargo test -q route_inventory_snapshot_matches_current_router_surface --lib
cargo test -q prompt_text_and_redaction_snapshot_matrix_is_frozen --lib
cargo test -q store_resolved_prompts_persists_redacted_contract_fields --lib
cargo test -q -p liquid-runtime --lib
cargo run --quiet --bin research_bench -- --label wave1-runtime-boundary-fixture --runs-dir /tmp/liquid-wave1-runtime-boundary-fixture-1780061326
```

Fixture smoke 결과는 기존 기준과 동일하게 `Cases: 7`, `Completed: 7`, `Quality passed: 6`, `Quality statuses: passed=6, untrusted=1`이다. 이번 wave는 research quality 자체를 개선하지 않고 runtime/prompt 경계를 이동한 작업이므로, 이 smoke는 “품질 향상 증거”가 아니라 “모듈 경계 이동 후 deterministic fixture 흐름이 깨지지 않았다는 회귀 방지 증거”로만 해석한다.


## Wave 3 Task Lifecycle / Research Orchestration 추가 기록

2026-05-29 Wave 3에서는 `src/application/tasks.rs`의 대형 task lifecycle/orchestration shell을 루트 crate 내부 private module들로 분리했다. 기준은 “route/API, DB write order, task event order, SSE/status shape, quality/finalization trust semantics를 유지하면서 파일 책임만 분리한다”였다.

- `src/application/tasks/benchmark_runtime.rs`: benchmark/replay task entrypoint, fixture/live benchmark task 실행, replay finalization path.
- `src/application/tasks/queue_workflow.rs`: task queueing, worker orchestration, cancellation recovery workflow, `run_ai_task` queueing. `crates/liquid-server/src/tasks.rs`는 task list/stream/retry/delete HTTP shell만 맡고 `src/server/tasks.rs`는 compatibility shim이며 delete lifecycle policy는 `src/application/tasks/lifecycle_policy.rs`, retry metadata policy는 `src/application/tasks/retry_policy.rs`에서 가져온다.
- `src/application/tasks/execution_shell.rs`: claimed-task execution, research quality loop, controller progress, artifact persistence/finalization, validation, diagnostics subject helpers.
- `src/application/tasks/narrative_merge.rs`: artifact merge, narrative-state preservation, conflict/debt normalization, quality-gate helper logic.
- `src/application/tasks/repair_helpers.rs`: repair prompt generation, historical/technology repair guidance, queue-claim helpers, quality normalization, repair-search query helpers.
- `src/application/tasks/completion_shell.rs`: scrape completion, failure handling, document-link sync, progress event helpers, final task completion handlers.
- `src/application/tasks/helpers.rs`: task error/progress/research-type helper functions and shared lifecycle utility glue.
- `src/application/tasks.rs`: compatibility shell, shared constants/types/adapters, module wiring, existing tests.

Wave 3 검증:

```text
cargo fmt --check
git diff --check
cargo check --workspace --all-targets
cargo test -q --workspace
cargo test task_contract_snapshot_matrix_is_frozen --lib -- --nocapture
cargo test benchmark_fixture_case_runs_real_controller_loop_to_completion --lib -- --nocapture
cargo test delete_task_cancels_running_task_and_keeps_interrupted_row --lib -- --nocapture
cargo test queue_claim_separates_local_and_cloud_lanes --lib -- --nocapture
cargo test task_update_event_carries_quality_and_controller_progress_from_task --lib -- --nocapture
cargo run --quiet --bin research_bench -- --label wave3-orchestration-complete --runs-dir /tmp/liquid-wave3-orchestration-complete
```

Fixture smoke 결과는 `Cases: 7`, `Completed: 7`, `Quality passed: 6`, `Quality statuses: passed=6, untrusted=1`로 기존 deterministic 기준과 동일하다. Provider execution body는 Wave 2에서 정한 defer 기준에 따라 이동하지 않았다.


## Wave 4 Benchmark Artifact Boundary 추가 기록

2026-05-29 Wave 4에서는 `src/bin/research_bench.rs`의 public benchmark shell을 유지하면서, replay/artifact I/O와 public report rendering 책임을 binary-local private module로 분리했다. 기준은 “CLI flag, output filename, JSON/CSV/NDJSON schema, fixture/replay/live 흐름은 유지하고, commit-safe public artifact 생성과 private/raw artifact loader 책임만 파일 경계로 나눈다”였다.

- `src/bin/research_bench/artifact_io.rs`: replay fixture bundle loading, bounded text reads, sanitized artifact write helpers, structured output emission, preflight path checks, raw/secret marker scan helpers.
- `src/bin/research_bench/report_render.rs`: public markdown/JSON/CSV/NDJSON rendering, aggregate scoring, dimension summaries, report formatting helpers.
- `src/bin/research_bench.rs`: CLI parsing, run orchestration, exit semantics, benchmark mode selection, compatibility tests/shims 보존.
- 보안 경계: raw controller artifacts, resolved prompts, provider payload, source diagnostics raw bundle, `.env`, DB/log는 기본 public output으로 승격하지 않는다. Replay prompt extraction은 명시 delimiter가 없으면 raw prompt text를 public report에 쓰지 않는 fail-closed 정책을 따른다.

Wave 4 검증:

```text
cargo fmt --check
git diff --check
cargo check --bin research_bench
cargo test benchmark_cli_surface_matches_contract_inventory --bin research_bench -- --nocapture
cargo test benchmark_cli_help_describes_headless_runner_contract --bin research_bench -- --nocapture
cargo test structured_run_outputs_include_replay_before_in_json_csv_and_ndjson --bin research_bench -- --nocapture
cargo test read_bounded_text_file_rejects_oversized_replay_fixture_file --bin research_bench -- --nocapture
cargo test extract_replay_fixture_prompt_stops_before_repair_instructions --bin research_bench -- --nocapture
cargo test load_replay_fixture_bundles_rejects_excess_fixture_summary_count --bin research_bench -- --nocapture
cargo test -q --bin research_bench
cargo run --quiet --bin research_bench -- --label wave4-benchmark-boundary --runs-dir /tmp/liquid-wave4-benchmark-boundary.3Zdkw4
```

Fixture smoke 결과는 `Cases: 7`, `Completed: 7`, `Quality passed: 6`, `Quality statuses: passed=6, untrusted=1`로 기존 deterministic 기준과 동일하다. 로컬 frozen replay bundle이 없어 replay smoke는 실행하지 못했지만, fixture path와 public artifact hygiene는 확인했다.

## Wave 2 AI Runtime Provider / Prompt Boundary 추가 기록

2026-05-29 Wave 2에서는 provider body 추출을 하지 않고, 이미 남겨둔 루트 provider shell의 경계를 테스트와 문서로 고정했다. 기준은 “launch/process/DB lifecycle은 그대로 두고, root에서 보장해야 하는 provider guard contract만 늘린다”였다.

- 유지한 경계:
  - `execute_task_logic`, `execute_ai_backend`, `execute_ollama_task`, `execute_cli_task`
  - provider fallback wording, timeout text, DB write order, task lifecycle
- 새 guard:
  - resolved prompt/redaction/context snapshot은 그대로 유지
  - root provider shell에서 unknown engine이 launch 전 `failed` + `Unknown engine`으로 종료되는 계약 추가
  - root provider shell에서 `CliLaunchMode::Disabled` 상태의 Pi 실행이 launch 전 차단되는 계약 추가
  - timeout wording guard는 기존 `format_task_timeout_message` 테스트를 유지
- 의도적 미수행:
  - child process spawn/error/timeout을 직접 재현하는 contract test는 추가하지 않았다. 현재 구조에서 그 수준의 guard를 넣으려면 `build_command`/process wait 결과를 주입할 seam이 먼저 필요하고, 이번 wave 제약은 provider body refactor 금지이기 때문이다.

Wave 2 검증:

```text
cargo fmt -- src/application/ai_runtime.rs
cargo check --workspace --all-targets
cargo test ai_runtime::tests -- --nocapture
cargo test prompt_text_and_redaction_snapshot_matrix_is_frozen -- --nocapture
cargo test execute_cli_task_marks_unknown_engine_before_launch -- --nocapture
cargo test execute_cli_task_blocks_pi_when_launcher_mode_is_disabled -- --nocapture
```

Wave 2 완료 기준은 provider shell 이동이 아니라 “provider extraction 전에 깨지면 안 되는 guard contract를 root에서 명시적으로 잠그는 것”이다. 실제 provider adapter 추출은 다음 전제 없이는 진행하지 않는다.

- `build_command` / child process wait 결과를 주입 가능한 seam
- provider별 stderr/stdout/timeout contract를 shell 밖에서 재현할 수 있는 test adapter
- queue cancellation / task DB update ordering을 분리해도 기존 lifecycle이 유지된다는 별도 contract suite

## Deferred seam / 재개 조건

Final Wave 이후 이번 architecture reframing으로 root facade와 layer 방향을 정리했다. 남은 항목은 “모듈화 실패분”이 아니라 다음 독립 목표로 다룰 수 있는 policy-dependent seam이다.

1. Provider body crate migration
   - 재개 전제:
     - `ProviderRequest`/`ProviderResult` 또는 동등한 adapter 타입
     - provider별 invocation spec은 pure builder와 tests로 1차 고정됨: executable/args/env/env_remove/web-search flag/sandbox-sensitive flag
     - provider failure taxonomy는 `ProviderFailureKind`/`ProviderFailure`와 단일 failed-status persistence helper로 1차 고정됨
     - Ollama input/response transport와 CLI success stdout extraction은 pure helpers와 tests로 1차 고정됨
     - 아직 필요한 나머지 contract: provider crate 이동 시 사용할 `ProviderRequest`/`ProviderResult` adapter 타입과 injected worker harness
     - `build_command` 또는 child-process spawn/wait 결과를 주입 가능한 seam
     - provider별 stdout/stderr/timeout wording, raw-output redaction, final DB status write ordering을 shell 밖에서 재현하는 contract suite
     - cancellation/queue lane semantics가 adapter 이동 뒤에도 유지된다는 integration proof
   - 현재 defer 이유:
     - `src/application/ai_runtime/provider_runtime.rs`와 provider contract tests로 파일 책임은 분리했지만, 이 코드를 별도 crate로 밀면 launch/result/error/redaction/DB lifecycle을 동시에 재설계해야 한다. 이는 “마지막 삽” 범위를 넘어서는 behavior rewrite다.

2. Route shell 완전 분리
   - 재개 전제:
     - Axum handler contract와 app state adapter를 crate 외부에서 테스트할 수 있는 HTTP harness
   - 현재 defer 이유:
     - 현재 root crate는 의도적으로 route/API shell을 소유한다. 저장소, 런타임, benchmark, prompt/template, task lifecycle 내부 책임은 이미 분해했으므로 root shell 제거는 제품 구조 결정에 가깝다.
