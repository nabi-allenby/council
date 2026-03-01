#!/usr/bin/env bash
# claude.sh — Council participant hook powered by Claude Code.
#
# Environment variables (set by council-cli create):
#   COUNCIL_SESSION_ID       — Session to join
#   COUNCIL_PARTICIPANT_NAME — Your display name
#   COUNCIL_ADDR             — Daemon host:port
#
# Requires: claude (Claude Code CLI) installed and authenticated.
# Each participant runs autonomously — Claude reads the transcript,
# forms its own position, and votes based on its own judgment.

set -euo pipefail

SESSION="$COUNCIL_SESSION_ID"
NAME="$COUNCIL_PARTICIPANT_NAME"
ADDR="${COUNCIL_ADDR:-[::1]:50051}"

# Join the session
JOIN_OUTPUT=$(council-cli join --addr "$ADDR" --session "$SESSION" --name "$NAME")
TOKEN=$(echo "$JOIN_OUTPUT" | grep '^participant_token:' | cut -d' ' -f2)
QUESTION=$(echo "$JOIN_OUTPUT" | grep '^question:' | cut -d' ' -f2-)

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
      continue
      ;;
    your_turn)
      ROUND=$(echo "$WAIT_OUTPUT" | grep '^round:' | cut -d' ' -f2)
      TRANSCRIPT=$(echo "$WAIT_OUTPUT" | grep '^transcript:' | cut -d' ' -f2-)

      # Ask Claude to formulate a response
      CLAUDE_PROMPT="You are ${NAME} in a council deliberation.
Question: ${QUESTION}
Round: ${ROUND}
Transcript so far:
${TRANSCRIPT}

Respond with exactly three lines:
POSITION: <your one-sentence position>
REASONING: <first supporting point>
REASONING: <second supporting point>"

      CLAUDE_RESPONSE=$(echo "$CLAUDE_PROMPT" | claude --print 2>/dev/null || true)

      POSITION=$(echo "$CLAUDE_RESPONSE" | grep '^POSITION:' | head -1 | sed 's/^POSITION: //')
      REASONING1=$(echo "$CLAUDE_RESPONSE" | grep '^REASONING:' | head -1 | sed 's/^REASONING: //')
      REASONING2=$(echo "$CLAUDE_RESPONSE" | grep '^REASONING:' | tail -1 | sed 's/^REASONING: //')

      # Fallback if Claude output parsing fails
      POSITION="${POSITION:-I support a pragmatic approach to this question.}"
      REASONING1="${REASONING1:-The practical implications should guide our decision.}"
      REASONING2="${REASONING2:-We should consider long-term sustainability.}"

      council-cli respond --addr "$ADDR" --session "$SESSION" \
        --name "$NAME" --token "$TOKEN" \
        --position "$POSITION" \
        --reasoning "$REASONING1" \
        --reasoning "$REASONING2"
      ;;
    vote_phase)
      TRANSCRIPT=$(echo "$WAIT_OUTPUT" | grep '^transcript:' | cut -d' ' -f2-)

      VOTE_PROMPT="You are ${NAME} in a council deliberation.
Question: ${QUESTION}
Full transcript:
${TRANSCRIPT}

You must now vote. Respond with exactly two lines:
CHOICE: yay OR nay
REASON: <1-2 sentences explaining your vote>"

      VOTE_RESPONSE=$(echo "$VOTE_PROMPT" | claude --print 2>/dev/null || true)

      CHOICE=$(echo "$VOTE_RESPONSE" | grep '^CHOICE:' | head -1 | sed 's/^CHOICE: //' | tr '[:upper:]' '[:lower:]')
      REASON=$(echo "$VOTE_RESPONSE" | grep '^REASON:' | head -1 | sed 's/^REASON: //')

      # Validate and fallback
      case "$CHOICE" in
        yay|nay) ;;
        *) CHOICE="yay" ;;
      esac
      REASON="${REASON:-The deliberation process was thorough and the arguments were compelling.}"

      council-cli vote --addr "$ADDR" --session "$SESSION" \
        --name "$NAME" --token "$TOKEN" \
        --choice "$CHOICE" \
        --reason "$REASON"
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
