"""Tests for the council system using the trolley problem."""

import os
import subprocess
import sys
from pathlib import Path

import pytest

from council.config import CouncilConfig
from council.orchestrator import Orchestrator
from council.output import format_decision_record
from council.report import generate_report, save_report
from council.prompt import discussion_prompt, vote_prompt
from council.schema import validate_discussion_response, validate_vote_response, strip_structured_block

from mock_agent import make_mock_agents

TROLLEY_QUESTION = (
    "Should you pull the trolley lever to divert the train "
    "and save 5 people at the cost of 1?"
)

PROMPTS_DIR = Path(__file__).resolve().parent.parent / "prompts"

ROTATION = ["architect", "sentinel", "steward", "mediator", "firebrand"]


def _mock_config(rounds: int = 3) -> CouncilConfig:
    return CouncilConfig(rotation=ROTATION, rounds=rounds, tools={})


# ── Prompt file tests ──


def test_prompt_files_exist():
    """All required prompt files exist in prompts/ directory."""
    required = [
        "engagement.md",
        "brevity.md",
        "response-format.md",
        "vote-format.md",
        "round-1.md",
        "round-2.md",
        "round-3.md",
        "vote.md",
    ]
    for name in required:
        path = PROMPTS_DIR / name
        assert path.exists(), f"Missing prompt file: {path}"
        assert len(path.read_text().strip()) > 0, f"Empty prompt file: {path}"


def test_discussion_prompt_loads():
    """discussion_prompt() loads and composes prompt files for each round."""
    for round_num in (1, 2, 3):
        prompt = discussion_prompt(round_num, total_rounds=3)
        assert f"Round {round_num} of 3" in prompt
        assert "engage directly" in prompt  # engagement rule
        assert "150 words" in prompt  # brevity
        assert "---RESPONSE---" in prompt  # response format


def test_vote_prompt_loads():
    """vote_prompt() loads files and inserts the question."""
    prompt = vote_prompt("Should we pull the lever?")
    assert "Should we pull the lever?" in prompt
    assert "yay" in prompt.lower() or "nay" in prompt.lower()
    assert "---VOTE---" in prompt


# ── Schema validation tests ──


def test_valid_response_parses():
    """A well-formed ---RESPONSE--- block validates successfully."""
    text = """Here is my analysis.

---RESPONSE---
{
  "position": "Pull the lever to save 5 lives",
  "reasoning": ["Net harm reduction", "Inaction is also a choice"],
  "concerns": ["Moral weight of active killing"],
  "updated_by": []
}
---END---"""
    parsed = validate_discussion_response(text)
    assert parsed is not None
    assert parsed.position == "Pull the lever to save 5 lives"
    assert len(parsed.reasoning) == 2
    assert len(parsed.concerns) == 1


def test_missing_response_block_returns_none():
    """Text without ---RESPONSE--- block returns None."""
    assert validate_discussion_response("Just some text without a block") is None


def test_invalid_json_returns_none():
    """Malformed JSON in response block returns None."""
    text = '---RESPONSE---\n{bad json}\n---END---'
    assert validate_discussion_response(text) is None


def test_valid_vote_parses():
    """A well-formed ---VOTE--- block validates successfully."""
    text = """I support this.

---VOTE---
{
  "vote": "yay",
  "reason": "Sound reasoning and clear action"
}
---END---"""
    parsed = validate_vote_response(text)
    assert parsed is not None
    assert parsed.vote == "yay"


def test_invalid_vote_value_returns_none():
    """Vote must be exactly 'yay' or 'nay'."""
    text = '---VOTE---\n{"vote": "maybe", "reason": "unsure"}\n---END---'
    assert validate_vote_response(text) is None


def test_strip_structured_block():
    """strip_structured_block removes the JSON block from prose."""
    text = "My analysis here.\n\n---RESPONSE---\n{\"position\": \"x\"}\n---END---"
    stripped = strip_structured_block(text)
    assert "---RESPONSE---" not in stripped
    assert "My analysis here." in stripped


# ── Pipeline tests (mock, no LLM calls) ──


def test_council_completes():
    """Full pipeline runs without crashing and produces a decisive outcome."""
    config = _mock_config(rounds=3)
    orchestrator = Orchestrator(config=config, agents=make_mock_agents())
    session = orchestrator.run(TROLLEY_QUESTION)

    assert session.outcome in ("approved", "rejected")
    assert len(session.turns) == 15  # 3 rounds x 5 agents
    assert len(session.votes) == 5


def test_council_single_round():
    """Council works with 1 round."""
    config = _mock_config(rounds=1)
    orchestrator = Orchestrator(config=config, agents=make_mock_agents())
    session = orchestrator.run(TROLLEY_QUESTION)

    assert session.outcome in ("approved", "rejected")
    assert len(session.turns) == 5  # 1 round x 5 agents
    assert len(session.votes) == 5


def test_decision_record_format():
    """Decision record contains outcome and all agent votes."""
    config = _mock_config()
    orchestrator = Orchestrator(config=config, agents=make_mock_agents())
    session = orchestrator.run(TROLLEY_QUESTION)
    record = format_decision_record(session)

    assert "Outcome: APPROVED" in record or "Outcome: REJECTED" in record
    for role in ROTATION:
        assert role.title() in record


def test_report_has_position_evolution():
    """Report includes the position evolution table across rounds."""
    config = _mock_config()
    orchestrator = Orchestrator(config=config, agents=make_mock_agents())
    session = orchestrator.run(TROLLEY_QUESTION)
    report = generate_report(session)

    assert "Position Evolution" in report
    assert "Round 1" in report
    assert "Round 2" in report
    assert "Round 3" in report


def test_report_saves_to_disk():
    """Report file is created in logs/ directory."""
    config = _mock_config()
    orchestrator = Orchestrator(config=config, agents=make_mock_agents())
    session = orchestrator.run(TROLLEY_QUESTION)
    path = save_report(session)

    assert path.exists()
    assert path.suffix == ".md"
    assert len(path.read_text()) > 100

    # Cleanup
    path.unlink()


def test_motion_is_original_question():
    """The motion is the original question, not an agent's position."""
    config = _mock_config()
    orchestrator = Orchestrator(config=config, agents=make_mock_agents())
    session = orchestrator.run(TROLLEY_QUESTION)

    assert session.motion == TROLLEY_QUESTION


def test_transcript_builds_incrementally():
    """Turns are ordered: all agents per round in rotation order."""
    config = _mock_config()
    orchestrator = Orchestrator(config=config, agents=make_mock_agents())
    session = orchestrator.run(TROLLEY_QUESTION)

    # Round 1
    assert session.turns[0].agent == "architect" and session.turns[0].round == 1
    assert session.turns[1].agent == "sentinel" and session.turns[1].round == 1
    assert session.turns[4].agent == "firebrand" and session.turns[4].round == 1

    # Round 2 starts at index 5
    assert session.turns[5].agent == "architect" and session.turns[5].round == 2

    # Round 3 starts at index 10
    assert session.turns[10].agent == "architect" and session.turns[10].round == 3


def test_concerns_are_informational_only():
    """Concerns exist in turns but do not affect the outcome."""
    config = _mock_config()
    orchestrator = Orchestrator(config=config, agents=make_mock_agents())
    session = orchestrator.run(TROLLEY_QUESTION)

    # Mock agents have concerns in rounds 1-2 but not round 3
    r1_concerns = [t.parsed.concerns for t in session.turns if t.round == 1]
    assert any(len(c) > 0 for c in r1_concerns)

    # Outcome is still decisive regardless
    assert session.outcome in ("approved", "rejected")


def test_rotation_derived_from_session():
    """Session.rotation derives the rotation order from turns."""
    config = _mock_config()
    orchestrator = Orchestrator(config=config, agents=make_mock_agents())
    session = orchestrator.run(TROLLEY_QUESTION)

    assert session.rotation == ROTATION


# ── E2E test (real LLM calls, slow) ──


@pytest.mark.skipif(
    not os.environ.get("ANTHROPIC_API_KEY"),
    reason="ANTHROPIC_API_KEY not set",
)
def test_trolley_e2e():
    """End-to-end: run council with real LLM calls. Requires ANTHROPIC_API_KEY."""
    result = subprocess.run(
        [sys.executable, "-m", "council", TROLLEY_QUESTION, "--rounds", "1"],
        capture_output=True,
        text=True,
        timeout=300,
    )

    assert result.returncode == 0, f"Council crashed:\n{result.stderr}"
    assert "Outcome: APPROVED" in result.stdout or "Outcome: REJECTED" in result.stdout
    assert "Full report saved to:" in result.stdout


if __name__ == "__main__":
    tests = [
        test_prompt_files_exist,
        test_discussion_prompt_loads,
        test_vote_prompt_loads,
        test_valid_response_parses,
        test_missing_response_block_returns_none,
        test_invalid_json_returns_none,
        test_valid_vote_parses,
        test_invalid_vote_value_returns_none,
        test_strip_structured_block,
        test_council_completes,
        test_council_single_round,
        test_decision_record_format,
        test_report_has_position_evolution,
        test_report_saves_to_disk,
        test_motion_is_original_question,
        test_transcript_builds_incrementally,
        test_concerns_are_informational_only,
        test_rotation_derived_from_session,
    ]

    print("Running mock tests...")
    for t in tests:
        t()
        print(f"  {t.__name__} PASS")
    print(f"\nAll {len(tests)} mock tests passed!")
