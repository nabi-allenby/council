"""Add tests/ directory to sys.path so mock_agent can be imported."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
