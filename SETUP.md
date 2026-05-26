# Setup

이 문서는 Liquid를 설치, 실행, 업데이트하는 절차를 정리합니다. 제품 소개와 사용 흐름은 [README.md](README.md)를 참고하세요.

## 요구 사항

- Rust stable toolchain
- Node.js
  - CI에서는 `node --check static/app.js`로 JavaScript 문법을 확인합니다.
- SQLite를 사용할 수 있는 OS 환경
- 선택 사항: Ollama
  - 로컬 Ollama 모델을 번역/조사 엔진으로 사용할 때 필요합니다.
- 선택 사항: `claude`, `gemini`, `codex` CLI
  - PATH에서 감지되고 CLI 런처가 실행 가능한 모드이면 앱의 모델 선택 목록에 나타납니다.
  - 기본 `auto` 모드는 macOS에서 `sandbox-exec`를 사용하고, WSL/Linux 등 비 macOS에서는 CLI/Pi 실행을 비활성화합니다.
  - WSL/Linux에서 CLI/Pi 엔진을 쓰려면 `LIQUID_CLI_LAUNCH_MODE=unsandboxed`를 명시해야 합니다.

## 설치

```bash
git clone https://github.com/c86j224s/liquid.git
cd liquid
cargo build
```

릴리즈 빌드가 필요하면:

```bash
cargo build --release
```

## 환경 변수

앱은 CLI 옵션과 환경 변수를 모두 지원합니다. 루트의 `.env.example`를 기준으로 `.env`를 만들 수 있고, `manage.sh`는 `.env`가 있을 때만 읽습니다.

| 환경 변수 | CLI 옵션 | 기본값 | 설명 |
| --- | --- | --- | --- |
| `LIQUID_DATA_DIR` | `--data-dir`, `-d` | `./data` | SQLite DB와 업로드 파일을 저장할 데이터 디렉터리 |
| `LIQUID_HOST` | `--host` | `127.0.0.1` | 바인딩할 호스트. 로컬 개발 기본값은 loopback이며, LAN/서버 노출이 필요할 때만 `0.0.0.0`으로 명시합니다. |
| `LIQUID_PORT` | `--port`, `-p` | `3000` | 웹 서버 포트 |
| `LIQUID_AI_WORKERS` | `--ai-workers` | `1` | 동시에 처리할 AI 작업 워커 수 |
| `LIQUID_LOCAL_AI_WORKERS` | `--local-ai-workers` | `1` | 로컬 모델 또는 로컬 실행 경로에 배정할 전용 워커 수 |
| `LIQUID_AI_TASK_TIMEOUT_SECS` | `--ai-task-timeout-secs` | `3600` | CLI/Pi 기반 AI 작업 하나가 실행될 수 있는 최대 시간. 기본 1시간, 최소 60초 |
| `LIQUID_CLI_LAUNCH_MODE` | `--cli-launch-mode` | `auto` | CLI/Pi 실행 방식. `auto`, `sandbox-exec`, `unsandboxed`, `disabled` 중 하나 |
| `LIQUID_RESEARCH_HTML_SKILL_PATH` | - | unset | 실험용 HTML 조사 디자인 스킬 파일 경로. 설정하면 HTML 조사 프롬프트에 해당 `SKILL.md` 내용을 추가하고, 읽기 실패 시 기존 프롬프트로 fallback |

루트의 `.env.example`는 다음 안전한 기본값을 담고 있습니다.

```env
LIQUID_HOST=127.0.0.1
LIQUID_PORT=3000
LIQUID_DATA_DIR=./data
LIQUID_AI_WORKERS=1
LIQUID_LOCAL_AI_WORKERS=1
LIQUID_AI_TASK_TIMEOUT_SECS=3600
LIQUID_CLI_LAUNCH_MODE=auto
```

`LIQUID_DATA_DIR`는 앱 시작 시 자동으로 생성됩니다. 운영 데이터는 코드 저장소 밖의 안정적인 경로에 두는 편이 좋습니다. `LIQUID_RESEARCH_HTML_SKILL_PATH` 같은 선택형 로컬 경로는 환경에 맞는 절대 경로로 직접 지정하세요.

로컬 단일 사용자 환경에서는 `LIQUID_HOST=127.0.0.1`를 유지하세요. 같은 네트워크의 다른 장치에서 접근해야 할 때만 `LIQUID_HOST=0.0.0.0`으로 바꾸고, 방화벽과 프록시 설정을 함께 검토하는 편이 안전합니다.

## 실행

개발 모드에서 바로 실행:

```bash
cargo run
```

환경 변수를 함께 지정:

```bash
LIQUID_DATA_DIR=~/data LIQUID_AI_WORKERS=2 LIQUID_AI_TASK_TIMEOUT_SECS=3600 cargo run
```

실행 후 기본 접속 주소:

```text
http://localhost:3000
```

## manage.sh

루트의 `manage.sh`는 `.env`를 읽어 서버를 백그라운드로 실행합니다.

```bash
./manage.sh start
./manage.sh status
./manage.sh logs
./manage.sh restart
./manage.sh stop
```

로그는 `server.log`, PID는 `.server.pid`에 기록됩니다. `manage.sh`로 실행한 서버는 시작/종료와 전달된 종료 신호를 `server.log`에 남기고, 다음 `start` 또는 `stop` 실행 시 stale PID 파일을 정리합니다.

서버 panic이 발생하면 앱은 `LIQUID_DATA_DIR` 아래 `crash-dumps/`에 panic payload, 발생 위치, 백트레이스를 포함한 crash report를 기록합니다. `SIGINT`, `SIGTERM`, `SIGHUP`은 graceful shutdown 신호로 처리되어 `server.log`에 남습니다. `SIGKILL`이나 시스템 강제 종료처럼 프로세스가 처리할 수 없는 종료는 앱 내부에서 기록할 수 없습니다.

## AI 엔진 준비

### Ollama

Ollama를 사용할 경우 Ollama 서버가 로컬에서 실행 중이어야 합니다. 앱은 `localhost:11434/api/generate`를 호출합니다.

```bash
ollama serve
ollama pull <model-name>
```

Pi+Ollama 엔진 프리셋에서 웹검색을 사용할 경우에도 Ollama web search API key는 필요하지 않습니다. Liquid는 Ollama를 로컬 모델 실행에만 사용하고, 웹검색/웹페치는 Pi 격리 런타임에 전달하는 Liquid 전용 web tool이 담당합니다.

설정 후 브라우저의 엔진 프리셋 설정 화면에서 Pi+Ollama 프리셋의 테스트 버튼을 누르면, Pi와 Ollama 모델 설정을 확인합니다.

### CLI 엔진

`claude`, `gemini`, `codex` 명령이 PATH에 있으면 앱이 자동으로 감지합니다.

- 연구 작업은 CLI 웹 검색을 사용할 수 있습니다.
- 번역과 문서 기반 작업은 선택한 문서 컨텍스트를 중심으로 실행됩니다.
- CLI 실행은 `LIQUID_CLI_LAUNCH_MODE` 정책을 따릅니다.
- `auto`: macOS에서는 제한된 `sandbox-exec` 설정으로 실행하고, 비 macOS에서는 CLI/Pi 실행을 비활성화합니다.
- `sandbox-exec`: macOS `sandbox-exec`를 명시적으로 요구합니다. 비 macOS에서는 사용할 수 없습니다.
- `unsandboxed`: CLI/Pi 명령을 OS 샌드박스 없이 직접 실행합니다. WSL/Linux 지원을 위한 명시적 opt-in이며, CLI 도구가 사용자 권한으로 파일/네트워크에 접근할 수 있습니다.
- `disabled`: CLI/Pi 실행을 비활성화합니다.
- Codex CLI는 외부 런처가 `unsandboxed`여도 기존 내부 옵션 `--sandbox read-only --ask-for-approval never`를 계속 사용합니다.

WSL/Linux에서 CLI 엔진을 명시적으로 허용하려면:

```bash
LIQUID_CLI_LAUNCH_MODE=unsandboxed cargo run
```

`unsandboxed`는 macOS `sandbox-exec`와 같은 격리를 제공하지 않습니다. 신뢰하는 로컬 환경에서만 사용하세요. 직접 Ollama 실행은 이 런처 정책을 거치지 않습니다.

## 데이터베이스와 파일

`LIQUID_DATA_DIR` 아래에 앱 데이터가 저장됩니다.

- SQLite DB: `liquid.db`
- 업로드/생성 파일: `uploads/`

앱 시작 시 `setup_db()`가 스키마를 확인하고 필요한 컬럼/테이블을 자동으로 보강합니다.

- `drawers` 테이블이 없으면 생성합니다.
- `tags`, `file_tags` 테이블이 없으면 생성합니다.
- `files.drawer_id` 컬럼이 없으면 추가합니다.
- 기존 제목의 `[AI-Research]`, `[Research]`, `[NO CONFIDENCE]` 같은 레거시 표식은 시작 마이그레이션에서 시스템 태그로 옮기고 제목은 순수 제목으로 정리합니다. 사용자 태그는 이 흐름에서 덮어쓰지 않습니다.
- published가 아닌 파일의 drawer 배정은 비웁니다.
- 시작 시 처리 중이던 작업은 `interrupted`로 정리되어 작업 목록에서 확인/재시도할 수 있습니다.

수동 마이그레이션 명령은 없습니다. 중요한 운영 데이터를 업데이트하기 전에는 `LIQUID_DATA_DIR` 전체를 백업하세요.

## 프로젝트 구조

백엔드는 단일 Rust 바이너리 크레이트입니다. `src/main.rs`는 실행 부트스트랩만 담당하고, 라우터/DB/모델/작업 큐/AI 실행/도메인별 핸들러는 `src/*.rs` 모듈에 나뉘어 있습니다.

프론트엔드는 번들러 없이 `static/`의 네이티브 ES 모듈을 사용합니다. `static/app.js`는 부트스트랩이고, 공유 상태는 `static/state.js`, UI/컨트롤러 로직은 `static/components/`와 `static/controllers/` 아래에 둡니다.

## 업데이트

일반적인 업데이트 순서:

```bash
./manage.sh stop
git switch main
git pull --ff-only origin main
cargo build
./manage.sh start
```

운영 데이터가 중요하다면 업데이트 전 백업:

```bash
cp -R ~/data ~/data.backup.$(date +%Y%m%d%H%M%S)
```

## 검증

로컬에서 변경을 확인할 때는 CI와 같은 명령을 실행합니다.

```bash
cargo check
cargo test
cargo build
node --check static/app.js
```

## 릴리즈 운영

이 저장소는 Release Please를 사용합니다.

- `feat:`는 minor 버전 후보를 만듭니다.
- `fix:`는 patch 버전 후보를 만듭니다.
- `feat!:` 또는 `fix!:`는 major 버전 후보를 만듭니다.

main에 Conventional Commit이 쌓이면 Release Please가 릴리즈 PR을 생성합니다. 릴리즈 PR을 머지하면 태그와 GitHub Release가 생성됩니다.

저장소 설정에서 GitHub Actions가 PR을 만들 수 있어야 합니다.

- Workflow permissions: read and write
- Allow GitHub Actions to create and approve pull requests

## 문제 해결

### 포트가 이미 사용 중입니다

다른 포트를 지정합니다.

```bash
LIQUID_PORT=3001 cargo run
```

또는 현재 3000번 포트 사용 프로세스를 확인합니다.

```bash
lsof -ti tcp:3000
```

### 모델 목록이 비어 있습니다

- Ollama가 실행 중인지 확인합니다.
- CLI 엔진을 쓰는 경우 `which claude`, `which gemini`, `which codex`로 PATH 등록을 확인합니다.

### Pi+Ollama 웹검색 도구 실행이 실패했습니다

Pi+Ollama 웹검색은 Ollama web search API가 아니라 Liquid 전용 Pi web tool을 사용합니다. 이 오류가 나오면 네트워크 연결, 검색 대상 사이트 응답, 또는 Pi extension 로딩을 확인합니다.

```bash
./manage.sh logs
```

### 조사 작업이 너무 오래 걸리거나 timeout 됩니다

CLI/Pi 기반 조사 작업의 기본 실행 제한은 1시간입니다. 복잡한 웹검색, 로컬 모델 추론, 긴 문서 생성이 겹치면 더 오래 걸릴 수 있습니다.

시간 제한을 늘리려면 `.env` 또는 실행 명령에 `LIQUID_AI_TASK_TIMEOUT_SECS`를 설정하고 서버를 재시작합니다.

```env
LIQUID_AI_TASK_TIMEOUT_SECS=3600
```

직접 Ollama HTTP 호출은 별도 내부 제한을 사용합니다. `LIQUID_AI_TASK_TIMEOUT_SECS`는 Pi+Ollama, Gemini, Claude, Codex 같은 CLI/Pi 실행 경로에 적용됩니다.

### 작업이 실패했습니다

앱의 작업 목록에서 실패 상세를 확인하거나 로그를 봅니다.

```bash
./manage.sh logs
```

실패 또는 interrupted 작업은 작업 목록에서 재시도할 수 있습니다.
