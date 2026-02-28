"""Mock agent that returns canned responses without LLM calls."""

from council.types import ParsedResponse, Turn, Vote

# Predefined positions per role
POSITIONS = {
    "creator": [
        "Pull the lever — net harm reduction of 4 lives is the strongest opening frame.",
        "Reframing: pulling the lever is an act of moral courage, not cold calculation.",
        "Pull the lever. The council converges — the question is about owning the choice.",
    ],
    "scout": [
        "Research supports pulling: ~90% of respondents choose to divert in survey studies.",
        "Dual-process theory confirms: deliberative reasoning favors utilitarian action here.",
        "Evidence is clear — pull the lever. Empirical and philosophical consensus align.",
    ],
    "skeptic": [
        "Pull, but the 5-vs-1 framing hides that you are actively choosing to kill one person.",
        "Creator and Scout overstate certainty — real trolley situations have incomplete info.",
        "Despite caveats, pulling is defensible. The Skeptic's job is stress-testing, not blocking.",
    ],
    "implementer": [
        "Pull the lever. Inaction lets 5 die — that is also a choice, and a worse one.",
        "Council aligns on pulling. Remaining disagreement is about framing, not the action.",
        "Pull the lever and own the moral weight. This is the council's clear recommendation.",
    ],
    "guardian": [
        "Pulling is values-aligned if you honor the gravity — the one person matters too.",
        "Skeptic's point about incomplete info is valid but doesn't change the answer here.",
        "Pull the lever. Acknowledge the cost. This is sustainable and ethically grounded.",
    ],
}

VOTE_REASONS = {
    "creator": "The motion captures both action and moral ownership — exactly right.",
    "scout": "Empirical evidence and philosophical consensus strongly support pulling.",
    "skeptic": "Despite reservations about framing, the substance is correct — pull.",
    "implementer": "Clear, actionable, and accounts for moral weight. Sound closure.",
    "guardian": "Values-aligned — pulling while owning the cost is the integrity move.",
}


class MockAgent:
    """Drop-in replacement for Agent that returns predefined responses."""

    def __init__(self, role: str):
        self.role = role
        self._call_count = 0

    def respond(self, round_num: int, system_context: str, messages: list[dict], **kwargs) -> Turn:
        idx = min(round_num - 1, len(POSITIONS[self.role]) - 1)
        position = POSITIONS[self.role][idx]

        return Turn(
            agent=self.role,
            round=round_num,
            content=f"[{self.role.title()} Round {round_num}] {position}",
            parsed=ParsedResponse(
                position=position,
                reasoning=[f"Reasoning point for {self.role} round {round_num}"],
                concerns=[] if round_num == 3 else [f"Minor caveat from {self.role}"],
                updated_by=[] if round_num == 1 else ["creator", "skeptic"],
            ),
        )

    def cast_vote(self, system_context: str, messages: list[dict], **kwargs) -> Vote:
        return Vote(
            agent=self.role,
            vote="yay",
            reason=VOTE_REASONS[self.role],
        )


def make_mock_agents() -> dict[str, MockAgent]:
    """Create a full set of mock agents for all 5 roles."""
    return {role: MockAgent(role) for role in POSITIONS}
