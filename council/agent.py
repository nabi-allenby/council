from pathlib import Path

import anthropic

from .schema import strip_structured_block, validate_discussion_response, validate_vote_response
from .types import Turn, Vote

MODEL = "claude-haiku-4-5-20251001"
MAX_TOKENS = 2048
MAX_TOKENS_SEARCH = 4096

WEB_SEARCH_TOOL = {
    "type": "web_search_20250305",
    "name": "web_search",
    "max_uses": 5,
}


class Agent:
    def __init__(self, role: str, personality_path: str):
        self.role = role
        self.personality = Path(personality_path).read_text()
        self.client = anthropic.Anthropic()

    def respond(
        self,
        round_num: int,
        system_context: str,
        messages: list[dict],
        max_retries: int = 2,
    ) -> Turn:
        system = self.personality + "\n\n" + system_context

        use_search = self.role == "scout"
        api_kwargs: dict = {
            "model": MODEL,
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
                    {
                        "role": "user",
                        "content": (
                            "Your response is missing or has an invalid ---RESPONSE--- block. "
                            "Please reply with ONLY the corrected block:\n\n"
                            "---RESPONSE---\n"
                            '{"position": "...", "reasoning": ["..."], '
                            '"concerns": [], "updated_by": []}\n'
                            "---END---"
                        ),
                    },
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
            "model": MODEL,
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
                    {
                        "role": "user",
                        "content": (
                            "Your response is missing or has an invalid ---VOTE--- block. "
                            "Please reply with ONLY the vote block:\n\n"
                            "---VOTE---\n"
                            '{"vote": "yay or nay", "reason": "one sentence"}\n'
                            "---END---"
                        ),
                    },
                ]

        raise ValueError(
            f"{self.role} failed to produce valid ---VOTE--- block "
            f"after {max_retries} retries"
        )

    @staticmethod
    def _extract_text(response) -> str:
        """Extract text from response, handling multi-block responses (e.g. web search).

        Web search responses interleave text blocks with tool-use/result blocks.
        Strip each block and rejoin with paragraph breaks, then collapse excess newlines.
        """
        parts = []
        for block in response.content:
            if hasattr(block, "text"):
                stripped = block.text.strip()
                if stripped:
                    parts.append(stripped)
        text = "\n\n".join(parts)
        while "\n\n\n" in text:
            text = text.replace("\n\n\n", "\n\n")
        return text
