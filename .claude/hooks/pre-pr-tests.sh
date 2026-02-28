#!/usr/bin/env bash
#
# Claude Code hook: runs the test suite before allowing PR creation.
# Configured as a PreToolUse hook on the Bash tool.
# Exit 0 = allow, Exit 2 = block (stderr shown to Claude).
#

set -euo pipefail

INPUT=$(cat)
COMMAND=$(echo "$INPUT" | jq -r '.tool_input.command // empty')

# Only gate on PR creation commands
if ! echo "$COMMAND" | grep -q "gh pr create"; then
  exit 0
fi

PROJECT_DIR=$(echo "$INPUT" | jq -r '.cwd // empty')
if [ -z "$PROJECT_DIR" ]; then
  PROJECT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
fi

echo "Pre-PR hook: running test suite before creating PR..." >&2

# Run Rust tests (exclude E2E / LLM tests)
TEST_OUTPUT=$(cargo test --manifest-path "$PROJECT_DIR/Cargo.toml" -- --skip e2e 2>&1) || {
  echo "BLOCKED: tests failed. Fix the failures before creating a PR." >&2
  echo "" >&2
  echo "$TEST_OUTPUT" >&2
  exit 2
}

echo "All tests passed." >&2
echo "$TEST_OUTPUT" >&2
exit 0
