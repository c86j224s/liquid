# Liquid 직관적 아키텍처 지도

작성일: 2026-05-30
브랜치: `refactor/modular-workspace-roadmap`

## 한 문장 원칙

Liquid의 구조는 **Core → Workflow → Adapters → Shell** 네 층으로 읽는다. 같은 규칙이 서버, 리서치, 런타임, 저장소, 벤치마크에 반복 적용된다.

## 네 가지 레이어

| 레이어 | 의미 | 현재 위치 | 규칙 |
| --- | --- | --- | --- |
| Core | I/O 없는 규칙, DTO, 품질 판정, 순수 변환. 단, `liquid-html-report`는 caller가 명시한 local HTML skill source를 읽는 bounded file policy까지 포함한다. | `crates/liquid-protocol`, `crates/liquid-files`, `crates/liquid-html-report`, `crates/liquid-research-core`, `crates/liquid-research-classic`, `crates/liquid-acquisition`의 pure helper, `crates/liquid-runtime`의 pure policy | SQL, Axum, AppState, provider process 실행을 모른다. |
| Workflow | 사용자가 요청한 일을 어떤 순서로 수행할지 결정 | `src/application`, 특히 `src/application/tasks.rs`, `src/application/ai_runtime.rs`, `crates/liquid-workspace` | route를 모르고, 외부 I/O는 ports/adapters를 통해 호출한다. `liquid-workspace`는 file/task/document-link use case를 재사용 가능한 workflow layer로 묶는다. |
| Adapters | DB, provider, filesystem, network, benchmark artifact 같은 외부 접점 구현 | `crates/liquid-storage-sqlite`, `crates/liquid-runtime`, `src/application/scraping.rs`의 guarded fetch, `crates/liquid-bench` | 외부 시스템과 대화하되 Core 규칙을 바꾸지 않는다. `liquid-storage-sqlite`는 `liquid-files` / `liquid-workspace` port 구현체다. |
| Shell | 실행 조립, HTTP route, startup, response mapping | `crates/liquid-server`, `src/server` shim. Root bootstrap/composition은 `src/main.rs`, `src/state.rs`가 맡는다. | 요청을 받고 Workflow를 호출하며, public response shape를 보존한다. |

루트 facade는 별도 레이어가 아니라 `src/contracts.rs`와 `src/lib.rs`가 묶어서 담당한다. `src/contracts.rs`는 root-local DTO/contract inventory를 모으고, `src/lib.rs`는 crate의 public re-export gate와 module wiring을 맡는다.

## Contract validation ownership

`liquid-protocol`은 Liquid 구성요소가 서로 주고받는 **wire-level protocol**을 맡는다. 그래서 제2의 앱이 Liquid 서버를 통하지 않고 같은 payload를 읽고 쓸 때도 동일해야 하는 DTO shape, serde default/alias, versioned artifact shape, wire value grammar를 둔다. 현재 공유된 값 집합은 파일 공개 상태(`draft|published|archived`), scrape mode(`general|geeknews`), research intensity(`low|medium|high`), research quality depth(`light|standard|strict`)이다.

반대로 Liquid 앱의 비즈니스 판단은 contract가 아니다. task 상태 전이, DB 존재 여부, provider 사용 가능 여부, SSRF/allowlist 정책, public artifact redaction, research quality gate처럼 AppState·DB·설정·보안 정책·workflow 단계에 의존하는 검사는 `src/application` 또는 `crates/liquid-server`에 남긴다. 공유 evidence URL trust policy는 `crates/liquid-research-core`의 absolute public URL helper에 두고, classic/app의 artifact merge, scaffold, repair, publication 경로는 모두 같은 helper를 재사용한다.

검토 기준은 세 문장으로 고정한다.

1. 이 규칙이 wire format/serialized payload의 문법이면 `crates/liquid-protocol`에 둔다.
2. Liquid의 저장소, 라우트, task lifecycle, provider, 보안 게시 정책에 의존하면 Liquid workflow/server에 둔다.
3. 둘 다 걸치면 contract의 순수 검사를 먼저 호출하고 Liquid 쪽에서 추가 정책만 덧붙인다.

`src/contracts.rs`는 protocol의 새 집이 아니라 root-local facade다. shared DTO/serde/wire grammar는 `liquid-protocol`이 소유하고, `src/contracts.rs`는 현재 Axum/UI shell이 쓰는 DTO 조립과 호환 re-export만 담당한다.

## 반복 패턴

앞으로 리팩토링과 리뷰에서 사용하는 패턴 이름은 세 가지뿐이다.

1. **freeze-then-move**
   - 먼저 현재 동작을 테스트로 고정하고 이동한다.
   - provider invocation, lifecycle status, public artifact sanitizer처럼 깨지면 위험한 영역에 적용한다.
2. **move-pure**
   - I/O 없는 규칙은 Core crate로 바로 이동한다.
   - 예: research quality, prompt/redaction helper, scraping string helper.
3. **move-with-trait**
   - I/O가 있는 코드는 request/result/trait contract를 먼저 만들고 adapter 구현으로 옮긴다.
   - 예: provider runner, SQLite repository, source acquisition.

이 세 가지에 맞지 않는 이동은 구조를 선명하게 하지 않는 것으로 보고 보류하거나 다시 설계한다.

## 현재 파일 매핑

### Shell — `crates/liquid-server` + root shim

| 파일 | 책임 |
| --- | --- |
| `crates/liquid-server/src/app.rs` | Axum route 등록, no-cache middleware, route inventory test |
| `crates/liquid-server/src/context.rs` | server-facing service ports, `ServerContext`, static asset config, HTTP shell contract boundary |
| `crates/liquid-server/src/presenters.rs` | public task summary shaping, rendered file page composition, verification appendix folding 같은 server-owned presentation policy |
| `crates/liquid-server/src/files.rs` | 파일/태그/관계/research-request HTTP response shell. `liquid-workspace`로 research-request/provenance use case를 위임하고, content render/upload/public summary trust boundary는 server에 남긴다. |
| `crates/liquid-server/src/files/research_public_summary.rs` | raw research diagnostics를 public-safe summary로 바꾸는 shell-facing trust boundary |
| `crates/liquid-server/src/research.rs` | research task submission route shell |
| `crates/liquid-server/src/tasks.rs` | task list/stream/retry/delete HTTP shell |
| `crates/liquid-server/src/scraping.rs` | `/api/scrap` HTTP handler and response mapping shell; URL normalization/GeekNews preparation/task enqueue는 `ServerContext` root adapter 뒤로 숨긴다 |
| `crates/liquid-server/src/engine_presets.rs` | engine preset HTTP handlers, request validation/status mapping; preset catalog/repository/status helpers are delegated to `src/application/engine_presets/`; pure preset policy comes from `crates/liquid-runtime` |
| `crates/liquid-server/src/drawers.rs` | drawer route shell |
| `crates/liquid-server/src/translate.rs` | translate route shell |
| `src/server/*.rs` | root compatibility shim; app-side tests and composition imports keep the old module path while production router lives in `liquid-server` |
| `src/lib.rs` | crate public export gate, module wiring, shared helper re-exports, contract inventory tests |

### Workflow — `src/application`

| 파일 | 책임 |
| --- | --- |
| `src/application/tasks.rs` | task workflow wiring, shared constants, compatibility exports, tests |
| `src/application/server_adapters.rs` | root composition-owned server service adapters over `AppState`; HTTP shell에 concrete storage/runtime를 직접 노출하지 않는 bridge |
| `src/application/tasks/runtime_adapters.rs` | task workflow가 쓰는 in-root repository/runtime/finalizer/source adapter wiring |
| `src/application/tasks/queue_workflow.rs` | task queueing, worker orchestration, cancellation recovery workflow |
| `src/application/tasks/execution_shell.rs` | claimed task execution, research quality loop, artifact persistence/finalization/validation orchestration, scrape/AI dispatch; pure local-pi scaffold policy는 classic boundary로 위임 |
| `src/application/tasks/completion_shell.rs` | output persistence, document link creation, completion handoff |
| `src/application/tasks/repair_helpers.rs` | queue/search/lifecycle helper와 classic repair policy compatibility shim |
| `src/application/tasks/narrative_merge.rs` | classic artifact-merge policy compatibility shim |
| `src/application/tasks/lifecycle_policy.rs` | delete/abort/queue lifecycle state vocabulary and cancellation policy |
| `src/application/tasks/retry_policy.rs` | retry eligibility and retry metadata reconstruction policy |
| `src/application/tasks/benchmark_runtime.rs` | benchmark fixture/replay workflow bridge used by `liquid-bench` compatibility |
| `src/application/ai_runtime.rs` | AI workflow entrypoint and provider dispatch decision |
| `src/application/engine_presets/` | engine preset catalog, repository facade, resolver, and runtime status checks |
| `src/application/ai_runtime/prompt_storage.rs` | resolved prompt storage redaction/persistence workflow helper |
| `src/application/ai_runtime/research_context_workflow.rs` | research context-pack preparation and raw fallback workflow helper; selected classic policy is injected through `ResearchImplementation` |
| `src/application/ai_runtime/translation_workflow.rs` | single-file KO translation and markdown chunking workflow helper |
| `src/application/ai_runtime/provider_runtime.rs` | provider execution adapter body guarded by invocation/failure/transport contracts |
| `crates/liquid-workspace` | file/task/document-link cross-entity use case: research-request assembly, source document hydration, task-output provenance/document-link orchestration |
| `src/application/scraping.rs` | scrape normalization, guarded fetch, extraction helpers; Shell의 HTTP/task enqueue mapping은 `crates/liquid-server/src/scraping.rs` |
| `src/application/research_prompts.rs` | classic prompt/template policy compatibility shim; delegates selected `ResearchImplementation` from `crates/liquid-research-classic` |
| `src/application/ports.rs` | in-root port traits for runtime/acquisition/finalizer/validator plus the app-facing `ResearchImplementation` seam |

### Core / Contracts — crates and root facade

| crate | 책임 |
| --- | --- |
| `crates/liquid-protocol` | shared DTOs and serialized artifact/API/benchmark protocol types |
| `crates/liquid-files` | file/content primitives, tag/domain helpers, document-link graph primitives, and file/tag/link port traits |
| `crates/liquid-html-report` | HTML-only report prompt policy, built-in interactive report guidance, and bounded local file/directory `SKILL.md` skill-source interpretation. The app chooses the path; this crate interprets the HTML report skill. It does not read env vars, AppState, DB, network, or research task state. |
| `crates/liquid-research-artifacts` | shared research artifact warning/detail vocabulary used by acquisition, research-core, and app repair flows |
| `crates/liquid-research-core` | research artifact parsing, validation, finalization, context rendering, quality rules, shared absolute public evidence URL trust policy |
| `crates/liquid-research-classic` | classic prompt/template policy, research mode/type normalization, web-search audit wording, context-pack planning, artifact merge/debt/narrative preservation, repair prompt shaping, and local-pi scaffold/provenance policy |
| `crates/liquid-acquisition` | source discovery models, source-pack construction, pure scraping/source helper logic |
| `crates/liquid-runtime` | CLI/Pi launcher support, engine preset pure policy, prompt redaction/translation helpers |
| `src/contracts.rs` | root-local DTO/contract facade that keeps the shared inventory adjacent to the root public API |

### Adapters — crates and boundary modules

| 위치 | 책임 |
| --- | --- |
| `crates/liquid-storage-sqlite` | SQLite schema, migrations, repository helpers, and `liquid-files` / `liquid-workspace` adapter implementations |
| `crates/liquid-runtime` | CLI/Pi runtime support and future provider adapter home |
| `crates/liquid-bench` | benchmark CLI, public/private artifact split, report rendering |
| `src/application/scraping.rs` guarded fetch portion | application-owned scrape adapter body behind a server route wrapper |

## 의존 방향

```text
Root bootstrap/composition(src/main.rs, src/state.rs)
  -> Shell(crates/liquid-server, src/server shim)
  -> ServerContext ports implemented by root adapters
  -> Workflow(src/application, crates/liquid-workspace)
  -> Core/Contracts(crates/liquid-files, crates/liquid-protocol, other crates/liquid-*)
  -> External systems only through Adapters

Adapters(crates/liquid-storage-sqlite, liquid-runtime, liquid-bench)
  -> Contracts/Core

Core
  -> liquid-protocol or std/workspace libs only
```

`liquid-html-report` is the one deliberate exception to “pure means no file read”: it may read only a caller-provided local HTML report skill source, bounded by size and explicit file/directory shape. The Liquid app remains responsible for env/config selection such as `LIQUID_RESEARCH_HTML_SKILL_PATH`; the crate remains responsible for interpreting that source as HTML report design policy. External skill text is not persisted verbatim in task prompt storage; persisted prompts keep a redaction marker and execution rehydrates from the configured source.

금지 방향:

- Core가 Shell을 참조하지 않는다.
- Core가 AppState, Axum, SQLx pool, provider process 실행을 알지 않는다.
- Shell-only route code가 research quality/prompt/provider/scrape 정책을 직접 새로 만들지 않는다.
- 예전 root shim 파일(`src/db.rs`, `src/research_quality.rs`, `src/research_sources.rs`, `src/cli_launcher.rs`, `src/pi_runtime.rs`)을 다시 만들지 않는다.

파일/워크스페이스 경계는 아래 패턴으로 읽는다.

```text
route shell
  -> workspace use case
  -> file primitives / file-tag-link ports
  -> sqlite adapter
```

이 경계에서 의도적으로 이동하지 않는 것:

- Axum route/response shape
- `AppState`
- uploads path와 filesystem read/write
- public sanitizer / `research_public_summary`
- scrape ownership
- translation ownership
- concrete research implementation

## 완료된 정리

- 예전 root re-export shim(`src/db.rs`, `src/research_quality.rs`, `src/research_sources.rs`, `src/cli_launcher.rs`, `src/pi_runtime.rs`)은 제거한 상태를 유지한다.
- production HTTP shell은 `crates/liquid-server`로 이동했고, `src/server/*.rs`는 기존 root import와 app-side test compatibility를 위한 얇은 re-export shim만 남긴다.
- task/AI/research-prompt/engine/scrape workflow/policy 파일을 `src/application` 아래로 모으고, HTTP handler는 `crates/liquid-server`에 고정했다.
- `src/application`이 `src/server`에 의존하지 않도록 방향을 고정했다.
- `src/models.rs`를 `src/contracts.rs`로 바꿔 root-local DTO facade라는 의미를 명확히 했다.
- 기존 public API, DB schema, benchmark CLI, prompt/quality behavior는 유지한다.

## 아직 같은 목표 안에서 남은 큰 경계

이 항목들은 별도 핑계가 아니라 다음 `freeze-then-move` 또는 `move-with-trait` 대상이다.

1. `src/application/ai_runtime/provider_runtime.rs`
   - provider body는 현재 invocation/failure/transport contract로 고정되어 있다. 별도 crate 이동은 behavior rewrite가 아니라 injected worker harness가 생길 때만 move-with-trait로 진행한다.
2. `src/application/tasks.rs`
   - workflow host가 여전히 크다. 다만 이 파일은 Shell이 아니라 Workflow로 분류되었으므로, 다음 분리는 “작게 만들기”가 아니라 use-case별 request/result 경계가 생길 때만 한다.
3. `crates/liquid-server/src/files.rs`
   - route shell과 일부 response assembly가 크다. 다만 이는 HTTP response shape와 public sanitizer를 보존하는 Shell 책임이며, persistence/workspace use case는 이미 `liquid-workspace`와 root adapter 뒤로 위임한다.
4. `src/application/scraping.rs`
   - guarded network fetch는 server wrapper 밖으로 이동했다. public URL/SSRF guard는 보존했고, future crate split은 move-with-trait 대상이다.
5. `crates/liquid-bench`
   - 아직 root `liquid` crate의 benchmark workflow re-export를 사용한다. 장기적으로는 Workflow contract를 직접 의존해야 한다.

## 완료 판정 기준

이 아키텍처가 완료됐다고 말하려면 다음이 모두 참이어야 한다.

- 파일 위치만 보고 Shell / Workflow / Core / Adapter 중 어디인지 알 수 있다.
- 새 코드가 세 패턴 중 하나로 설명된다.
- 예전 root shim이 되살아나지 않고, 남은 `src/server` shim은 compatibility-only re-export로 제한된다.
- 큰 파일이 남더라도 그 이유가 레이어와 책임으로 설명된다.
- 전체 workspace test와 route/benchmark/quality/lifecycle contract가 통과한다.
