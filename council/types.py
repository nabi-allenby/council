from dataclasses import dataclass, field
from typing import Literal


@dataclass
class ParsedResponse:
    """Validated structured data from a discussion turn."""

    position: str
    reasoning: list[str]
    concerns: list[str] = field(default_factory=list)
    updated_by: list[str] = field(default_factory=list)


@dataclass
class ParsedVote:
    """Validated structured data from a vote turn."""

    vote: Literal["yay", "nay"]
    reason: str


@dataclass
class Turn:
    agent: str  # "creator" | "scout" | "skeptic" | "implementer" | "guardian"
    round: int  # 1, 2, or 3
    content: str  # Prose only (structured block stripped)
    parsed: ParsedResponse


@dataclass
class Vote:
    agent: str
    vote: Literal["yay", "nay"]
    reason: str


@dataclass
class Session:
    question: str
    turns: list[Turn] = field(default_factory=list)
    votes: list[Vote] = field(default_factory=list)

    @property
    def outcome(self) -> Literal["approved", "rejected"]:
        yays = sum(1 for v in self.votes if v.vote == "yay")
        return "approved" if yays >= 3 else "rejected"

    @property
    def motion(self) -> str:
        """The position being voted on (Implementer's round 3 position)."""
        for turn in reversed(self.turns):
            if turn.agent == "implementer" and turn.round == 3:
                return turn.parsed.position
        # Fallback: last turn's position
        return self.turns[-1].parsed.position if self.turns else ""
