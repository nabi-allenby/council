#!/usr/bin/env bash
# hook.sh — Generic council participant hook.
#
# Environment variables (set by council-cli create):
#   COUNCIL_SESSION_ID       — Session to join
#   COUNCIL_PARTICIPANT_NAME — Your display name
#   COUNCIL_ADDR             — Daemon host:port
#   COUNCIL_AGENT_COMMAND    — Agent command (from config.toml [agent] command)
#
# Optional:
#   COUNCIL_AGENT_FILE       — Path to personality .md file
#
# The agent command receives a prompt on stdin and must print a response
# on stdout. Example: "claude -p", "cat" (echo), custom scripts.

set -euo pipefail

SESSION="$COUNCIL_SESSION_ID"
NAME="$COUNCIL_PARTICIPANT_NAME"
ADDR="${COUNCIL_ADDR:-[::1]:50051}"
AGENT_CMD="${COUNCIL_AGENT_COMMAND:?COUNCIL_AGENT_COMMAND is required}"

# Load agent personality if provided
PERSONALITY=""
if [ -n "${COUNCIL_AGENT_FILE:-}" ] && [ -f "$COUNCIL_AGENT_FILE" ]; then
  PERSONALITY=$(cat "$COUNCIL_AGENT_FILE")
fi

# Create temp files once; clean up on exit
CONV_FILE=$(mktemp)
VOTE_FILE=$(mktemp)
cleanup() { rm -f "$CONV_FILE" "$VOTE_FILE"; }
trap cleanup EXIT

# Join the session
JOIN_OUTPUT=$(council-cli join --addr "$ADDR" --session "$SESSION" --name "$NAME")
TOKEN=$(echo "$JOIN_OUTPUT" | grep '^participant_token:' | cut -d' ' -f2)
QUESTION=$(echo "$JOIN_OUTPUT" | grep '^question:' | cut -d' ' -f2-)

if [ -z "$TOKEN" ]; then
  echo "Failed to join session" >&2
  exit 1
fi

# Seed conversation file with personality (if provided)
if [ -n "$PERSONALITY" ]; then
  cat > "$CONV_FILE" <<EOF
${PERSONALITY}

---

EOF
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
      # Transcript is multi-line; capture from the transcript: line to the end
      TRANSCRIPT=$(echo "$WAIT_OUTPUT" | awk '/^transcript: /{found=1; sub(/^transcript: /, ""); print; next} found{print}')

      # Append this round's prompt to the conversation file.
      # The file accumulates across rounds so the agent sees its own
      # prior reasoning alongside the server transcript.
      cat >> "$CONV_FILE" <<EOF
=== ROUND $ROUND PROMPT ===
You are ${NAME} in a council deliberation.
Question: ${QUESTION}
Round: ${ROUND}
Transcript so far:
${TRANSCRIPT}

Respond with exactly three lines:
POSITION: <your one-sentence position>
REASONING: <first supporting point>
REASONING: <second supporting point>
EOF

      POSITION=""
      REASONING1=""
      REASONING2=""
      ATTEMPT=0
      MAX_RETRIES=3
      while [ $ATTEMPT -lt $MAX_RETRIES ]; do
        ATTEMPT=$((ATTEMPT + 1))
        RESPONSE=$(cat "$CONV_FILE" | $AGENT_CMD 2>/dev/null || true)

        # Append agent response to conversation history
        cat >> "$CONV_FILE" <<EOF
=== YOUR RESPONSE (ROUND $ROUND, ATTEMPT $ATTEMPT) ===
$RESPONSE
=== END YOUR RESPONSE ===
EOF

        POSITION=$(echo "$RESPONSE" | grep '^POSITION:' | head -1 | sed 's/^POSITION: //')
        REASONING1=$(echo "$RESPONSE" | grep '^REASONING:' | head -1 | sed 's/^REASONING: //')
        REASONING_COUNT=$(echo "$RESPONSE" | grep -c '^REASONING:' || true)
        if [ "$REASONING_COUNT" -ge 2 ]; then
          REASONING2=$(echo "$RESPONSE" | grep '^REASONING:' | sed -n '2p' | sed 's/^REASONING: //')
        else
          REASONING2=""
        fi

        if [ -n "$POSITION" ] && [ -n "$REASONING1" ]; then
          break
        fi
        echo "Attempt $ATTEMPT/$MAX_RETRIES: agent output unparseable, retrying..." >&2
      done

      if [ -z "$POSITION" ] || [ -z "$REASONING1" ]; then
        echo "ERROR: agent failed to produce parseable output after $MAX_RETRIES attempts" >&2
        exit 1
      fi

      # Build reasoning args — only include second if distinct
      REASONING_ARGS=(--reasoning "$REASONING1")
      if [ -n "$REASONING2" ]; then
        REASONING_ARGS+=(--reasoning "$REASONING2")
      fi

      council-cli respond --addr "$ADDR" --session "$SESSION" \
        --name "$NAME" --token "$TOKEN" \
        --position "$POSITION" \
        "${REASONING_ARGS[@]}"
      ;;
    vote_phase)
      TRANSCRIPT=$(echo "$WAIT_OUTPUT" | awk '/^transcript: /{found=1; sub(/^transcript: /, ""); print; next} found{print}')

      # Build vote prompt in a separate file (personality + vote instructions)
      if [ -n "$PERSONALITY" ]; then
        cat > "$VOTE_FILE" <<EOF
${PERSONALITY}

---

EOF
      else
        : > "$VOTE_FILE"
      fi
      cat >> "$VOTE_FILE" <<EOF
You are ${NAME} in a council deliberation.
Question: ${QUESTION}
Full transcript:
${TRANSCRIPT}

You must now vote. Respond with exactly two lines:
CHOICE: yay OR nay
REASON: <1-2 sentences explaining your vote>
EOF

      CHOICE=""
      REASON=""
      ATTEMPT=0
      MAX_RETRIES=3
      while [ $ATTEMPT -lt $MAX_RETRIES ]; do
        ATTEMPT=$((ATTEMPT + 1))
        VOTE_RESPONSE=$(cat "$VOTE_FILE" | $AGENT_CMD 2>/dev/null || true)

        cat >> "$VOTE_FILE" <<EOF
=== YOUR RESPONSE (VOTE, ATTEMPT $ATTEMPT) ===
$VOTE_RESPONSE
=== END YOUR RESPONSE ===
EOF

        CHOICE=$(echo "$VOTE_RESPONSE" | grep '^CHOICE:' | head -1 | sed 's/^CHOICE: //' | tr '[:upper:]' '[:lower:]')
        REASON=$(echo "$VOTE_RESPONSE" | grep '^REASON:' | head -1 | sed 's/^REASON: //')

        case "$CHOICE" in
          yay|nay) ;;
          *) CHOICE="" ;;
        esac

        if [ -n "$CHOICE" ] && [ -n "$REASON" ]; then
          break
        fi
        echo "Attempt $ATTEMPT/$MAX_RETRIES: vote output unparseable, retrying..." >&2
      done

      if [ -z "$CHOICE" ] || [ -z "$REASON" ]; then
        echo "ERROR: agent failed to produce parseable vote after $MAX_RETRIES attempts" >&2
        exit 1
      fi

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
