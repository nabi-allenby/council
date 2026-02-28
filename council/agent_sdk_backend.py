"""Agent backend using the Claude Agent SDK for multi-turn tool use."""

from __future__ import annotations

import asyncio
import os
from pathlib import Path

from .agent_base import DISCUSSION_RETRY_PROMPT, VOTE_RETRY_PROMPT, normalize_text
from .schema import strip_structured_block, validate_discussion_response, validate_vote_response
from .types import Turn, Vote

# Map council tool names to Agent SDK tool names
TOOL_MAP = {
    "web_search": "WebSearch",
}

MAX_TURNS_DEFAULT = 1
MAX_TURNS_WITH_TOOLS = 5


class AgentSDKAgent:
    """Agent that uses the Claude Agent SDK instead of direct API calls.

    Provides the same respond() and cast_vote() interface as Agent,
    but leverages the Agent SDK for multi-turn tool use and richer
    orchestration primitives.
    """

    def __init__(self, role: str, personality_path: str, model: str, tools: list[str] | None = None):
        self.role = role
        self.personality = Path(personality_path).read_text()
        self.model = model
        self.tools = tools or []
        self.sdk_tools = [TOOL_MAP[t] for t in self.tools if t in TOOL_MAP]

    def respond(
        self,
        round_num: int,
        system_context: str,
        messages: list[dict],
        max_retries: int = 2,
    ) -> Turn:
        system = self.personality + "\n\n" + system_context
        prompt = messages[0]["content"]

        for attempt in range(max_retries + 1):
            text = self._run_query(system, prompt)
            parsed = validate_discussion_response(text)

            if parsed is not None:
                return Turn(
                    agent=self.role,
                    round=round_num,
                    content=strip_structured_block(text),
                    parsed=parsed,
                )

            if attempt < max_retries:
                prompt = f"{prompt}\n\n{DISCUSSION_RETRY_PROMPT}"

        raise ValueError(
            f"{self.role} failed to produce valid ---RESPONSE--- block "
            f"after {max_retries} retries"
        )

    def cast_vote(
        self,
        system_context: str,
        messages: list[dict],
        max_retries: int = 2,
    ) -> Vote:
        system = self.personality + "\n\n" + system_context
        prompt = messages[0]["content"]

        for attempt in range(max_retries + 1):
            text = self._run_query(system, prompt)
            parsed = validate_vote_response(text)

            if parsed is not None:
                return Vote(
                    agent=self.role,
                    vote=parsed.vote,
                    reason=parsed.reason,
                )

            if attempt < max_retries:
                prompt = f"{prompt}\n\n{VOTE_RETRY_PROMPT}"

        raise ValueError(
            f"{self.role} failed to produce valid ---VOTE--- block "
            f"after {max_retries} retries"
        )

    def _run_query(self, system: str, prompt: str) -> str:
        """Run a query through the Agent SDK and return the text response."""
        # The Agent SDK spawns a Claude Code subprocess. If we're already inside
        # a Claude Code session, the CLAUDECODE env var triggers a nesting guard.
        # Temporarily remove it so the subprocess can launch cleanly.
        saved = os.environ.pop("CLAUDECODE", None)
        try:
            return asyncio.run(self._async_query(system, prompt))
        finally:
            if saved is not None:
                os.environ["CLAUDECODE"] = saved

    async def _async_query(self, system: str, prompt: str) -> str:
        from claude_agent_sdk import (
            query,
            ClaudeAgentOptions,
            AssistantMessage,
            TextBlock,
            ResultMessage,
        )

        options = ClaudeAgentOptions(
            model=self.model,
            system_prompt=system,
            allowed_tools=self.sdk_tools if self.sdk_tools else [],
            max_turns=MAX_TURNS_WITH_TOOLS if self.sdk_tools else MAX_TURNS_DEFAULT,
            permission_mode="bypassPermissions",
        )

        parts: list[str] = []
        result_text: str | None = None

        async for message in query(prompt=prompt, options=options):
            if isinstance(message, AssistantMessage):
                for block in message.content:
                    if isinstance(block, TextBlock):
                        stripped = block.text.strip()
                        if stripped:
                            parts.append(stripped)
            elif isinstance(message, ResultMessage) and message.result:
                result_text = message.result

        if result_text:
            return result_text

        return normalize_text("\n\n".join(parts))
