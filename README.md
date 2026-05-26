# Liquid

Liquid는 읽고, 모으고, 번역하고, 다시 생각하기 위한 로컬 지식 라이브러리입니다.

이 저장소는 운영 중인 내부 저장소에서 정리한 sanitized snapshot입니다. 실행에 필요한 소스, 설정 예시, 공개 가능한 문서만 포함하며, 로컬 데이터와 민감한 연구 산출물은 제외했습니다.

웹에서 발견한 글을 스크랩하고, 필요한 경우 한국어로 번역하고, 여러 문서를 묶어 심층 조사 리포트로 확장합니다. 결과물은 바로 공개하지 않고 먼저 draft로 쌓입니다. 읽어보고 쓸 만한 것만 published로 올리고, published 지식은 서랍(drawer)에 넣어 주제별로 정리합니다.

## 어떤 도구인가요?

Liquid는 브라우저에서 쓰는 개인용 지식 작업대입니다.

- 좋은 글을 스크랩하고 원문 출처와 참고 링크를 함께 보관합니다.
- 영어 문서를 스크랩하면서 한국어 번역본을 draft로 만들 수 있습니다.
- 이미 가진 문서를 바탕으로 번역, 단일 문서 조사, 여러 문서 융합 조사를 실행합니다.
- 새 주제를 바로 조사해 Markdown 또는 인터랙티브 HTML 리포트로 남깁니다.
- published 지식은 서랍에 넣고, draft와 archived는 작업 흐름에서 분리해 관리합니다.
- 콘텐츠 뷰어에서는 원본 복사, 맨 위/맨 아래 이동, Markdown 표와 코드 블록 렌더링을 지원합니다.
- 오래 걸리는 AI 작업은 큐에 들어가고, 작업 목록에서 상태와 실패 원인을 확인할 수 있습니다.

## 작업 흐름

```text
발견한 글
  -> 스크랩 / 스크랩 + 번역
  -> draft로 저장
  -> 읽고 다듬기
  -> published로 올리기
  -> 서랍에 정리
  -> 필요 없어진 지식은 archived로 이동
```

AI가 만든 결과도 원본을 덮어쓰지 않습니다. 번역, 조사, 스크랩 결과는 새 draft로 생성되므로, 사람이 마지막에 판단하고 정리하는 흐름을 유지합니다.

## 주요 기능

- **Knowledge Library**: Published, Draft, Archived, All 상태별 지식 보기
- **Drawers**: published 지식을 주제별 서랍에 배정
- **Scrape**: 웹 문서를 Markdown으로 저장, 참고 링크 별첨
- **Scrape + Translate**: 스크랩 후 한국어 번역 draft 생성
- **Research**: 문서 기반 조사, 여러 문서 융합 조사, 새 주제 조사. `[Research]`와 `[AI-Research]` 작업은 사용자 화면에서 같은 `Research` 시스템 태그로 표시되며, 조사 요청 상세에는 남아 있는 출처 문서가 클릭 가능한 링크로 표시됩니다. 삭제되었거나 찾을 수 없는 출처는 저장된 파일명 텍스트로 남깁니다. 고강도 조사 작업은 Source Cards, Claim Log, Conflict Map, Research Debt, 그리고 진단 요약을 함께 남기며, 파일 상세에서는 민감한 URL을 가린 요약만 노출합니다.
- **Research Benchmarks**: `docs/experiments/research-richness/README.md`와 `docs/experiments/research-richness/improvement-run.sh`로 fixture, replay, bounded live gate 흐름과 commit-safe 산출물 보존 규칙을 확인할 수 있습니다.
- **Document lineage**: 완료된 작업과 후속 작업 출력은 출처 문서와의 직접 관계를 유지합니다. 뷰어는 현재 문서에서 출처 링크를 바로 열 수 있게 보여 주고, 번역/후속 조사/융합 조사 결과도 별도 draft로 남깁니다.
- **Viewer**: Markdown/HTML 보기, 원본 복사, 빠른 위/아래 이동
- **Task Queue**: 스크랩, 번역, 조사 작업을 큐로 처리하고 실시간 상태 표시
- **Model Engines**: Ollama 모델과 `claude`, `gemini`, `codex` CLI 엔진 감지. CLI/Pi 실행은 macOS에서 기본적으로 `sandbox-exec`를 사용하며, WSL/Linux에서는 `LIQUID_CLI_LAUNCH_MODE=unsandboxed`를 명시한 경우에만 실행됩니다.

## 설치와 운영

설치, 실행, 업데이트, 환경 변수, AI 엔진 준비는 [SETUP.md](SETUP.md)에 따로 정리했습니다.

이미 준비된 환경이라면:

```bash
./manage.sh start
```

기본 접속 주소는 `http://localhost:3000`입니다.

## 릴리즈

이 저장소는 Conventional Commits와 Release Please로 버전을 관리합니다. 변경 기록은 [CHANGELOG.md](CHANGELOG.md)를 확인하세요.

## 라이선스 상태

현재 이 저장소에는 공개 라이선스가 부여되어 있지 않습니다. 별도 라이선스 문서가 추가되기 전까지는 모든 권리가 보유됩니다.
