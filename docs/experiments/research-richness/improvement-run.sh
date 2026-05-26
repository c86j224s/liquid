#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

MODE="fixture"
LABEL="quality-improvement-$(date -u +%Y%m%dT%H%M%SZ)"
CASE_DIR="$SCRIPT_DIR/cases"
RUN_DIR=""
PRESERVE_DIR=""
REPLAY_FIXTURE_ROOT=""
RESEARCH_INTENSITY="${RESEARCH_INTENSITY:-high}"
QUALITY_DEPTH="${QUALITY_DEPTH:-strict}"
MAX_ITERATIONS="${MAX_ITERATIONS:-2}"

usage() {
  cat <<'EOF'
Usage: improvement-run.sh [options]

Runs a reusable research-quality improvement measurement and optionally
copies only commit-safe artifacts into a durable review directory.

Options:
  --mode MODE                 fixture, replay, or live (default: fixture)
  --dry-run                   Alias for --mode fixture
  --label NAME                Run label (default: quality-improvement-<UTC>)
  --cases-dir DIR             Case directory for fixture/live modes
  --runs-dir DIR              Raw run artifact directory (default: temp dir)
  --replay-fixture-root DIR   Frozen replay bundle root for replay mode
  --preserve-dir DIR          Copy durable csv/sanitized final-output artifacts here
  --research-intensity VALUE  Forwarded to research_bench (default: high)
  --quality-depth VALUE       Forwarded to research_bench (default: strict)
  --max-iterations VALUE      Forwarded to research_bench (default: 2)
  -h, --help                  Show this help

Environment:
  MODEL_INPUT                 Required for live mode, for example cli:codex
  ENGINE_NAME                 Optional report metadata label
  MODEL_NAME                  Optional report metadata label
  LIQUID_BENCH_DATA_DIR       Optional base data dir for isolated case runs

Preservation policy:
  Copies only <label>.csv and sanitized *-final-output.md files.
  Writes a preservation note. Leaves aggregate markdown, raw JSON, NDJSON,
  embedded artifact JSON, source diagnostics, controller artifacts, and
  resolved prompts in the raw run directory.
EOF
}

validate_label() {
  local label="$1"
  if [[ ! "$label" =~ ^[A-Za-z0-9._-]+$ || "$label" == *".."* ]]; then
    echo "Invalid label: use only A-Z, a-z, 0-9, dot, underscore, or hyphen without '..'" >&2
    exit 1
  fi
}

require_dir() {
  local label="$1"
  local path="$2"
  if [[ ! -d "$path" ]]; then
    echo "$label does not exist or is not a directory: $path" >&2
    exit 1
  fi
}

sanitize_final_output() {
  local source="$1"
  local target="$2"

  awk '
    /^\[RESEARCH_ARTIFACT_JSON\][[:space:]]*$/ {
      dropping = 1
      fence_count = 0
      next
    }
    dropping {
      if ($0 ~ /^```/) {
        fence_count++
        if (fence_count >= 2) {
          dropping = 0
        }
      }
      next
    }
    { print }
  ' "$source" > "$target"
}

assert_preserved_final_output_is_sanitized() {
  local path="$1"
  if grep -Eq '\[RESEARCH_ARTIFACT_JSON\]|"source_cards"|"claim_log"|"research_debt"|"diagnostics_ref"|resolved-system-prompt|source-diagnostics|controller-artifacts' "$path"; then
    echo "Preserved final output still contains raw artifact markers: $path" >&2
    exit 1
  fi
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --mode)
      MODE="${2:?missing mode}"
      shift 2
      ;;
    --dry-run)
      MODE="fixture"
      shift
      ;;
    --label)
      LABEL="${2:?missing label}"
      shift 2
      ;;
    --cases-dir)
      CASE_DIR="${2:?missing cases dir}"
      shift 2
      ;;
    --runs-dir)
      RUN_DIR="${2:?missing runs dir}"
      shift 2
      ;;
    --replay-fixture-root)
      REPLAY_FIXTURE_ROOT="${2:?missing replay fixture root}"
      shift 2
      ;;
    --preserve-dir)
      PRESERVE_DIR="${2:?missing preserve dir}"
      shift 2
      ;;
    --research-intensity)
      RESEARCH_INTENSITY="${2:?missing research intensity}"
      shift 2
      ;;
    --quality-depth)
      QUALITY_DEPTH="${2:?missing quality depth}"
      shift 2
      ;;
    --max-iterations)
      MAX_ITERATIONS="${2:?missing max iterations}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

case "$MODE" in
  fixture|replay|live) ;;
  *)
    echo "Invalid mode: $MODE. Expected fixture, replay, or live." >&2
    exit 1
    ;;
esac

validate_label "$LABEL"

if [[ -z "$RUN_DIR" ]]; then
  RUN_DIR="$(mktemp -d "${TMPDIR:-/tmp}/research-quality-improvement.XXXXXX")"
fi
mkdir -p "$RUN_DIR"

if [[ "$MODE" == "live" && -z "${MODEL_INPUT:-}" ]]; then
  echo "Live mode requires MODEL_INPUT, for example MODEL_INPUT=cli:codex." >&2
  exit 1
fi
if [[ "$MODE" == "replay" ]]; then
  if [[ -z "$REPLAY_FIXTURE_ROOT" ]]; then
    echo "Replay mode requires --replay-fixture-root." >&2
    exit 1
  fi
  require_dir "Replay fixture root" "$REPLAY_FIXTURE_ROOT"
else
  require_dir "Cases dir" "$CASE_DIR"
fi

cmd=(
  cargo run --quiet --bin research_bench --
  --mode "$MODE"
  --label "$LABEL"
  --runs-dir "$RUN_DIR"
  --research-intensity "$RESEARCH_INTENSITY"
  --quality-depth "$QUALITY_DEPTH"
  --max-iterations "$MAX_ITERATIONS"
)

if [[ "$MODE" == "replay" ]]; then
  cmd+=(--replay-fixture-root "$REPLAY_FIXTURE_ROOT")
else
  cmd+=(--cases-dir "$CASE_DIR")
fi
if [[ -n "${LIQUID_BENCH_DATA_DIR:-}" ]]; then
  cmd+=(--data-dir "$LIQUID_BENCH_DATA_DIR")
fi
if [[ -n "${MODEL_INPUT:-}" ]]; then
  cmd+=(--model-input "$MODEL_INPUT")
fi
if [[ -n "${ENGINE_NAME:-}" ]]; then
  cmd+=(--engine-name "$ENGINE_NAME")
fi
if [[ -n "${MODEL_NAME:-}" ]]; then
  cmd+=(--model-name "$MODEL_NAME")
fi

echo "Running research quality improvement measurement:"
printf '  %q' "${cmd[@]}"
echo

(
  cd "$REPO_ROOT"
  "${cmd[@]}"
)

echo "Raw run artifacts: $RUN_DIR"

if [[ -n "$PRESERVE_DIR" ]]; then
  mkdir -p "$PRESERVE_DIR/runs/$LABEL"

  if [[ -f "$RUN_DIR/$LABEL.csv" ]]; then
    cp "$RUN_DIR/$LABEL.csv" "$PRESERVE_DIR/runs/$LABEL.csv"
  fi

  if [[ -d "$RUN_DIR/$LABEL" ]]; then
    while IFS= read -r -d '' file; do
      preserved_file="$PRESERVE_DIR/runs/$LABEL/$(basename "$file")"
      sanitize_final_output "$file" "$preserved_file"
      assert_preserved_final_output_is_sanitized "$preserved_file"
    done < <(find "$RUN_DIR/$LABEL" -type f -name '*-final-output.md' -print0)
  fi

  cat > "$PRESERVE_DIR/$LABEL-preservation-note.md" <<EOF
# $LABEL Preservation Note

This directory contains only durable research-quality improvement artifacts:

- \`runs/$LABEL.csv\` when the compact score table exists
- \`runs/$LABEL/*-final-output.md\` sanitized final reader-facing outputs

Aggregate markdown, raw JSON, NDJSON, machine-readable artifact blocks, source
diagnostics, controller artifacts, resolved prompts, and provider payloads
remain in the raw run directory:

\`$RUN_DIR\`
EOF

  echo "Commit-safe artifacts copied to: $PRESERVE_DIR"
fi
