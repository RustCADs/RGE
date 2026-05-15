#!/usr/bin/env bash
set -euo pipefail

mkdir -p .ai

if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  echo "Not inside a Git repository" >&2
  exit 1
fi

if [ ! -f .ai/claude_brief.schema.json ]; then
  echo "Missing .ai/claude_brief.schema.json" >&2
  exit 1
fi

if [ ! -f .ai/codex_review.schema.json ]; then
  echo "Missing .ai/codex_review.schema.json" >&2
  exit 1
fi

git diff > .ai/current.diff

if [ ! -s .ai/current.diff ]; then
  echo "No uncommitted diff to review."
  exit 0
fi

echo "Creating Claude brief..."
claude -p \
  --output-format json \
  --json-schema "$(cat .ai/claude_brief.schema.json)" \
  "Analyze .ai/current.diff and produce a concise review brief for Codex. Focus on correctness, regressions, security, test gaps, and edge cases." \
  > .ai/claude_brief.envelope.json

jq '.structured_output' .ai/claude_brief.envelope.json > .ai/claude_brief.json

echo "Running Codex review..."
codex exec \
  --sandbox read-only \
  --output-schema .ai/codex_review.schema.json \
  --output-last-message .ai/codex_review.json \
  "Review the repository and .ai/current.diff. Use .ai/claude_brief.json as context. Return schema-compliant JSON only. Do not edit files."

echo "Review result:"
jq . .ai/codex_review.json

VERDICT=$(jq -r '.verdict' .ai/codex_review.json)

case "$VERDICT" in
  pass)
    echo "Codex verdict: pass"
    ;;
  needs_changes)
    echo "Codex verdict: needs_changes"
    exit 2
    ;;
  block)
    echo "Codex verdict: block"
    exit 3
    ;;
  *)
    echo "Unknown Codex verdict: $VERDICT" >&2
    exit 4
    ;;
esac
