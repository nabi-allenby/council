import argparse
import os
import sys

from dotenv import load_dotenv

from .config import CouncilConfig, load_config
from .orchestrator import Orchestrator
from .output import format_decision_record
from .report import save_report


def main() -> None:
    load_dotenv()

    parser = argparse.ArgumentParser(
        prog="council",
        description="Run a council discussion on a question using AI agents",
    )
    parser.add_argument("question", help="The question for the council to discuss")
    parser.add_argument(
        "--verbose", "-v", action="store_true", help="Show turns and votes during execution"
    )
    parser.add_argument(
        "--rounds", "-r", type=int, default=None, help="Number of discussion rounds (overrides config)"
    )
    parser.add_argument(
        "--rotation", type=str, default=None,
        help="Comma-separated agent rotation order (overrides config), e.g. architect,firebrand,steward"
    )
    parser.add_argument(
        "--tools", type=str, default=None,
        help="Comma-separated agent:tool pairs (overrides config), e.g. architect:web_search,scout:web_search"
    )
    args = parser.parse_args()

    if not os.environ.get("ANTHROPIC_API_KEY"):
        print(
            "Error: ANTHROPIC_API_KEY is not set.\n"
            "Add it to a .env file:\n"
            "  echo 'ANTHROPIC_API_KEY=your-key-here' > .env\n"
            "Or export it directly:\n"
            "  export ANTHROPIC_API_KEY='your-key-here'",
            file=sys.stderr,
        )
        sys.exit(1)

    # Load config from agents/council.json
    try:
        config = load_config()
    except (FileNotFoundError, ValueError) as e:
        print(f"Error loading config: {e}", file=sys.stderr)
        sys.exit(1)

    # Apply CLI overrides
    if args.rounds is not None:
        config.rounds = args.rounds
    if args.rotation is not None:
        config.rotation = [r.strip() for r in args.rotation.split(",")]
    if args.tools is not None:
        config.tools = {}
        for pair in args.tools.split(","):
            agent, tool = pair.strip().split(":")
            config.tools.setdefault(agent.strip(), []).append(tool.strip())

    try:
        orchestrator = Orchestrator(config=config, verbose=args.verbose)
    except FileNotFoundError as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)

    try:
        session = orchestrator.run(args.question)
    except ValueError as e:
        print(f"Error during council discussion: {e}", file=sys.stderr)
        sys.exit(1)

    # Print concise decision to stdout
    print(format_decision_record(session))

    # Save full report to logs/
    report_path = save_report(session)
    print(f"\nFull report saved to: {report_path}")


if __name__ == "__main__":
    main()
