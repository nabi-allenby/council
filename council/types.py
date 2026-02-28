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
    agent: str
    round: int
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
        return "approved" if yays >= len(self.votes) // 2 + 1 else "rejected"

    @property
    def motion(self) -> str:
        """The motion is the original question put to vote."""
        return self.question

    @property
    def rotation(self) -> list[str]:
        """Derive rotation order from turns."""
        seen: list[str] = []
        for turn in self.turns:
            if turn.agent not in seen:
                seen.append(turn.agent)
        return seen
