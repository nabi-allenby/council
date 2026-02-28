from __future__ import annotations

from pathlib import Path

import anthropic

from .agent_base import DISCUSSION_RETRY_PROMPT, VOTE_RETRY_PROMPT, normalize_text
from .schema import strip_structured_block, validate_discussion_response, validate_vote_response
from .types import Turn, Vote

MAX_TOKENS = 2048
MAX_TOKENS_SEARCH = 4096

WEB_SEARCH_TOOL = {
    "type": "web_search_20250305",
    "name": "web_search",
    "max_uses": 5,
}


class Agent:
    def __init__(self, role: str, personality_path: str, model: str, tools: list[str] | None = None):
        self.role = role
        self.personality = Path(personality_path).read_text()
        self.model = model
        self.tools = tools or []
        self.client = anthropic.Anthropic()

    def respond(
        self,
        round_num: int,
        system_context: str,
        messages: list[dict],
        max_retries: int = 2,
    ) -> Turn:
        system = self.personality + "\n\n" + system_context

        use_search = "web_search" in self.tools
        api_kwargs: dict = {
            "model": self.model,
            "max_tokens": MAX_TOKENS_SEARCH if use_search else MAX_TOKENS,
            "system": system,
            "messages": messages,
        }
        if use_search:
            api_kwargs["tools"] = [WEB_SEARCH_TOOL]

        for attempt in range(max_retries + 1):
            response = self.client.messages.create(**api_kwargs)
            text = self._extract_text(response)
            parsed = validate_discussion_response(text)

            if parsed is not None:
                return Turn(
                    agent=self.role,
                    round=round_num,
                    content=strip_structured_block(text),
                    parsed=parsed,
                )

            if attempt < max_retries:
                messages = messages + [
                    {"role": "assistant", "content": text},
                    {"role": "user", "content": DISCUSSION_RETRY_PROMPT},
                ]

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

        api_kwargs: dict = {
            "model": self.model,
            "max_tokens": 512,
            "system": system,
            "messages": messages,
        }

        for attempt in range(max_retries + 1):
            response = self.client.messages.create(**api_kwargs)
            text = self._extract_text(response)
            parsed = validate_vote_response(text)

            if parsed is not None:
                return Vote(
                    agent=self.role,
                    vote=parsed.vote,
                    reason=parsed.reason,
                )

            if attempt < max_retries:
                messages = messages + [
                    {"role": "assistant", "content": text},
                    {"role": "user", "content": VOTE_RETRY_PROMPT},
                ]

        raise ValueError(
            f"{self.role} failed to produce valid ---VOTE--- block "
            f"after {max_retries} retries"
        )

    @staticmethod
    def _extract_text(response: anthropic.types.Message) -> str:
        """Extract text from response, handling multi-block responses (e.g. web search)."""
        parts: list[str] = []
        for block in response.content:
            if hasattr(block, "text"):
                stripped = block.text.strip()
                if stripped:
                    parts.append(stripped)
        return normalize_text("\n\n".join(parts))
