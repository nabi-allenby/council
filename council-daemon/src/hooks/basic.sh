#!/usr/bin/env bash
# basic.sh — Minimal council participant hook.
#
# Environment variables (set by council-cli create):
#   COUNCIL_SESSION_ID       — Session to join
#   COUNCIL_PARTICIPANT_NAME — Your display name
#   COUNCIL_ADDR             — Daemon host:port
#
# This hook joins a session and participates with simple echo responses.
# Replace the respond/vote logic with your own (LLM call, script, etc.).

set -euo pipefail

SESSION="$COUNCIL_SESSION_ID"
NAME="$COUNCIL_PARTICIPANT_NAME"
ADDR="${COUNCIL_ADDR:-[::1]:50051}"

# Join the session
JOIN_OUTPUT=$(council-cli join --addr "$ADDR" --session "$SESSION" --name "$NAME")
TOKEN=$(echo "$JOIN_OUTPUT" | grep '^participant_token:' | cut -d' ' -f2)

if [ -z "$TOKEN" ]; then
  echo "Failed to join session" >&2
  exit 1
fi

# Participation loop
while true; do
  WAIT_OUTPUT=$(council-cli wait --addr "$ADDR" --session "$SESSION" \
    --name "$NAME" --token "$TOKEN" --timeout 30)
  STATUS=$(echo "$WAIT_OUTPUT" | grep '^status:' | cut -d' ' -f2)

  case "$STATUS" in
    lobby|waiting)
      # Not our turn yet, loop
      continue
      ;;
    your_turn)
      ROUND=$(echo "$WAIT_OUTPUT" | grep '^round:' | cut -d' ' -f2)
      council-cli respond --addr "$ADDR" --session "$SESSION" \
        --name "$NAME" --token "$TOKEN" \
        --position "I have reviewed the discussion for round $ROUND." \
        --reasoning "Based on the arguments presented so far." \
        --reasoning "My analysis considers practical trade-offs."
      ;;
    vote_phase)
      council-cli vote --addr "$ADDR" --session "$SESSION" \
        --name "$NAME" --token "$TOKEN" \
        --choice yay \
        --reason "The discussion reached a reasonable consensus."
      ;;
    complete)
      council-cli results --addr "$ADDR" --session "$SESSION"
      exit 0
      ;;
    *)
      echo "Unexpected status: $STATUS" >&2
      exit 1
      ;;
  esac
done
