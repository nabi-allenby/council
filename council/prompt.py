from pathlib import Path

PROMPTS_DIR = Path(__file__).resolve().parent.parent / "prompts"


def _load(name: str) -> str:
    return (PROMPTS_DIR / name).read_text().strip()


def discussion_prompt(round_num: int, total_rounds: int) -> str:
    """Generate the system context for a discussion round."""
    round_guidance = _load(f"round-{round_num}.md")
    engagement = _load("engagement.md")
    brevity = _load("brevity.md")
    response_format = _load("response-format.md")

    return (
        f"You are participating in a council discussion, Round {round_num} of {total_rounds}.\n\n"
        f"{round_guidance}\n\n"
        f"{engagement}\n\n"
        f"{brevity}\n\n"
        f"{response_format}"
    )


def vote_prompt(question: str) -> str:
    """Generate the system context for the vote round."""
    template = _load("vote.md")
    engagement = _load("engagement.md")
    vote_format = _load("vote-format.md")

    return (
        f"{template.replace('{question}', question)}\n\n"
        f"{engagement}\n\n"
        f"{vote_format}"
    )
