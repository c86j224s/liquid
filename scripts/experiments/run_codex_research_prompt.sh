#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 3 ]]; then
  echo "usage: $0 PROMPT_FILE CASE_FILE OUTPUT_FILE" >&2
  exit 2
fi

PROMPT_FILE="$1"
CASE_FILE="$2"
OUTPUT_FILE="$3"
BASE_PROMPT="$(dirname "$PROMPT_FILE")/prompt-v5.md"

mkdir -p "$(dirname "$OUTPUT_FILE")"

{
  if [[ "$(basename "$PROMPT_FILE")" =~ ^prompt-v([6-9]|1[0-5])\.md$ && -f "$BASE_PROMPT" ]]; then
    cat "$BASE_PROMPT"
    printf '\n\n# Additional Iteration Rules\n\n'
  fi
  cat "$PROMPT_FILE"
  printf '\n\n# Benchmark Case\n\n'
  cat "$CASE_FILE"
} | codex exec \
  --ephemeral \
  --sandbox read-only \
  --skip-git-repo-check \
  --output-last-message "$OUTPUT_FILE" \
  -
