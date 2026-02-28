"""Mock agent that returns canned responses without LLM calls."""

from __future__ import annotations

from council.types import ParsedResponse, Turn, Vote

# Predefined positions per role
POSITIONS = {
    "architect": [
        "Reframe: this is not a binary — it is a design question about what kind of moral agent you want to be.",
        "The lever pull is the obvious answer; the interesting question is why we built systems where this choice exists.",
        "Pull the lever. But the elegant move is preventing this scenario entirely — that is the real design challenge.",
    ],
    "sentinel": [
        "Pull, but the moral injury to the actor is being underweighted — someone has to live with actively causing a death.",
        "The Architect's reframe dodges the immediate stakes. Real people die while we redesign systems.",
        "Pull the lever, carry the cost. But flag: repeated trolley choices erode moral sensitivity over time.",
    ],
    "steward": [
        "Pull the lever. 5 > 1. But document the reasoning and assign accountability for preventing recurrence.",
        "Architect and Sentinel both have points, but neither has a concrete next step. Here is what we actually do.",
        "Pull. Then: incident review within 48 hours, infrastructure audit within 30 days, assigned owner for each.",
    ],
    "mediator": [
        "Everyone is converging on pulling — the real disagreement is about what happens after and how we hold the cost.",
        "Architect and Steward are saying the same thing differently: act now, fix systems later. The Sentinel adds the emotional cost.",
        "Pull the lever. The group agrees. Name the cost, resource the follow-through, support the person who acts.",
    ],
    "firebrand": [
        "Pull the lever. Five lives outweigh one. Stop philosophizing and decide.",
        "The council is overthinking this. The math is clear and the moral case holds. Pull it.",
        "Pull the lever and own the choice. This is the council's clear recommendation.",
    ],
}

VOTE_REASONS = {
    "architect": "The reframe holds: act now, design the prevention. This is the right shape.",
    "sentinel": "Pull, but only because the alternative is worse. The cost to the actor is real.",
    "steward": "Clear, accountable, actionable. Pull the lever with follow-through.",
    "mediator": "The group converged honestly. Pull the lever, hold the cost together.",
    "firebrand": "Five lives. One lever. Pull it. This should not have taken three rounds.",
}


class MockAgent:
    """Drop-in replacement for Agent that returns predefined responses."""

    def __init__(self, role: str, tools: list[str] | None = None):
        self.role = role
        self.tools = tools or []

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
                updated_by=[] if round_num == 1 else ["architect", "sentinel"],
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
