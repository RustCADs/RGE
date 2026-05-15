#!/usr/bin/env bash
set -euo pipefail

cd "${CLAUDE_PROJECT_DIR:-.}"
mkdir -p .ai

cat > .ai/last_claude_hook_event.json

if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  exit 0
fi

git diff > .ai/latest.diff
if [ ! -s .ai/latest.diff ]; then
  exit 0
fi

if [ ! -f .ai/codex_review.schema.json ]; then
  echo "Missing .ai/codex_review.schema.json" >&2
  exit 0
fi

if codex exec \
  --sandbox read-only \
  --output-schema .ai/codex_review.schema.json \
  --output-last-message .ai/codex_last_review.json \
  "Review .ai/latest.diff for correctness, regressions, security bugs, race conditions, data loss, and test gaps. Return schema-compliant JSON only. Do not edit files."; then
  SUMMARY=$(jq -r '.summary // "Codex review completed."' .ai/codex_last_review.json 2>/dev/null || echo "Codex review completed.")
  jq -nc --arg msg "Codex review finished: $SUMMARY. Full review: .ai/codex_last_review.json" \
    '{hookSpecificOutput: {hookEventName: "PostToolUse", additionalContext: $msg}}'
else
  jq -nc --arg msg "Codex review failed. Check .ai/ and Claude hook debug logs." \
    '{systemMessage: $msg}'
fi
