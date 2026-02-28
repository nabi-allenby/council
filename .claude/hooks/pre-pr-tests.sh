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

# Find a working python with pytest installed
VENV_PYTHON="$PROJECT_DIR/.venv/bin/python"
if [ -x "$VENV_PYTHON" ] && "$VENV_PYTHON" -m pytest --version >/dev/null 2>&1; then
  PYTHON="$VENV_PYTHON"
elif command -v python3 >/dev/null 2>&1 && python3 -m pytest --version >/dev/null 2>&1; then
  PYTHON="python3"
elif [ -x /tmp/council-test-venv/bin/python ]; then
  PYTHON="/tmp/council-test-venv/bin/python"
else
  echo "WARNING: pytest not found — skipping pre-PR tests." >&2
  exit 0
fi

# Run tests (exclude E2E / LLM tests)
TEST_OUTPUT=$("$PYTHON" -m pytest "$PROJECT_DIR/tests/" -x -q -k "not e2e" 2>&1) || {
  echo "BLOCKED: tests failed. Fix the failures before creating a PR." >&2
  echo "" >&2
  echo "$TEST_OUTPUT" >&2
  exit 2
}

echo "All tests passed." >&2
echo "$TEST_OUTPUT" >&2
exit 0
