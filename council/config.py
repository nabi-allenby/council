"""Load and validate council configuration from agents/council.json."""

import json
from dataclasses import dataclass, field
from pathlib import Path


AGENTS_DIR = Path(__file__).resolve().parent.parent / "agents"
CONFIG_FILE = "council.json"


DEFAULT_MODEL = "claude-haiku-4-5-20251001"
VALID_BACKENDS = ("api", "agent-sdk")


@dataclass
class CouncilConfig:
    rotation: list[str]
    rounds: int = 3
    model: str = DEFAULT_MODEL
    tools: dict[str, list[str]] = field(default_factory=dict)
    backend: str = "api"


def load_config(agents_dir: Path = AGENTS_DIR) -> CouncilConfig:
    """Load council.json from agents directory and validate it."""
    config_path = agents_dir / CONFIG_FILE
    if not config_path.exists():
        raise FileNotFoundError(f"Config file not found: {config_path}")

    data = json.loads(config_path.read_text())

    rotation = data.get("rotation")
    if not isinstance(rotation, list) or len(rotation) < 1:
        raise ValueError("Config 'rotation' must be a list of at least 1 agent name")
    if not all(isinstance(r, str) for r in rotation):
        raise ValueError("Config 'rotation' entries must be strings")
    if len(rotation) > 7:
        raise ValueError("Config 'rotation' must have at most 7 agents")
    if len(rotation) > 1 and len(rotation) % 2 == 0:
        raise ValueError("Config 'rotation' must have an odd number of agents (or exactly 1)")

    rounds = data.get("rounds", 3)
    if not isinstance(rounds, int) or rounds < 1 or rounds > 3:
        raise ValueError("Config 'rounds' must be an integer between 1 and 3")

    model = data.get("model", DEFAULT_MODEL)
    if not isinstance(model, str) or not model.strip():
        raise ValueError("Config 'model' must be a non-empty string")

    backend = data.get("backend", "api")
    if backend not in VALID_BACKENDS:
        raise ValueError(f"Config 'backend' must be one of {VALID_BACKENDS}, got: {backend!r}")

    tools = data.get("tools", {})
    if not isinstance(tools, dict):
        raise ValueError("Config 'tools' must be a mapping of agent name to tool list")

    # Validate agent personality files exist
    for role in rotation:
        path = agents_dir / f"{role}.md"
        if not path.exists():
            raise FileNotFoundError(f"Agent personality file not found: {path}")

    # Validate tools keys reference agents in rotation
    for role in tools:
        if role not in rotation:
            raise ValueError(f"Config 'tools' references unknown agent: {role}")

    return CouncilConfig(rotation=rotation, rounds=rounds, model=model, tools=tools, backend=backend)
