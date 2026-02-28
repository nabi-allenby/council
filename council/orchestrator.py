from pathlib import Path

from .agent import Agent
from .config import CouncilConfig
from .prompt import discussion_prompt, vote_prompt
from .types import Session, Turn, Vote

AGENTS_DIR = Path(__file__).resolve().parent.parent / "agents"


class Orchestrator:
    def __init__(
        self,
        config: CouncilConfig,
        agents_dir: Path = AGENTS_DIR,
        verbose: bool = False,
        agents: dict[str, Agent] | None = None,
    ):
        self.config = config
        self.verbose = verbose
        if agents is not None:
            self.agents = agents
        else:
            self.agents: dict[str, Agent] = {}
            for role in config.rotation:
                path = agents_dir / f"{role}.md"
                if not path.exists():
                    raise FileNotFoundError(f"Agent personality file not found: {path}")
                tools = config.tools.get(role, [])
                self.agents[role] = Agent(role, str(path), tools=tools)

    def run(self, question: str) -> Session:
        session = Session(question=question)

        # --- Discussion: N rounds × M agents ---
        for round_num in range(1, self.config.rounds + 1):
            self._log_round(round_num)
            system_ctx = discussion_prompt(round_num, self.config.rounds)

            for role in self.config.rotation:
                self._log_waiting(role)
                transcript = self._build_transcript(session, round_num, role)
                messages = [{"role": "user", "content": transcript}]

                turn = self.agents[role].respond(round_num, system_ctx, messages)
                session.turns.append(turn)
                self._log_turn(turn)

        # --- Vote ---
        self._log_round("VOTE")
        vote_ctx = vote_prompt(question)
        full_transcript = self._build_full_transcript(session)

        for role in self.config.rotation:
            self._log_waiting(role, "voting")
            vote_message = (
                f"{full_transcript}\n\n---\n\n"
                f"You are {role.title()}. Cast your vote on the question above."
            )
            messages = [{"role": "user", "content": vote_message}]

            vote = self.agents[role].cast_vote(vote_ctx, messages)
            session.votes.append(vote)
            self._log_vote(vote)

        return session

    def _build_transcript(
        self, session: Session, current_round: int, current_role: str
    ) -> str:
        """Build the transcript visible to a specific agent at a specific point."""
        parts = [f"# Question\n\n{session.question}"]

        prev_round = 0
        for turn in session.turns:
            if turn.round != prev_round:
                prev_round = turn.round
                parts.append(f"## Round {turn.round}")

            entry = f"### {turn.agent.title()} (Round {turn.round})\n\n{turn.content}"
            entry += f"\n\n**Position:** {turn.parsed.position}"
            if turn.parsed.concerns:
                entry += f"\n**Concerns:** {'; '.join(turn.parsed.concerns)}"

            parts.append(entry)

        parts.append(
            f"\n---\n\nYou are **{current_role.title()}** speaking in "
            f"**Round {current_round}**."
        )

        return "\n\n".join(parts)

    def _build_full_transcript(self, session: Session) -> str:
        """Build the complete transcript for the vote phase."""
        parts = [
            f"# Question\n\n{session.question}",
            "# Full Discussion Transcript",
        ]

        prev_round = 0
        for turn in session.turns:
            if turn.round != prev_round:
                prev_round = turn.round
                parts.append(f"## Round {prev_round}")

            entry = f"### {turn.agent.title()}\n\n{turn.content}"
            entry += f"\n\n**Position:** {turn.parsed.position}"
            if turn.parsed.concerns:
                entry += f"\n**Concerns:** {'; '.join(turn.parsed.concerns)}"

            parts.append(entry)

        return "\n\n".join(parts)

    def _log_waiting(self, role: str, action: str = "thinking") -> None:
        if self.verbose:
            print(f"  [{role.title()} {action}...]", flush=True)

    def _log_round(self, round_id: int | str) -> None:
        if self.verbose:
            print(f"\n{'=' * 60}", flush=True)
            print(f"  ROUND: {round_id}", flush=True)
            print(f"{'=' * 60}\n", flush=True)

    def _log_turn(self, turn: Turn) -> None:
        if self.verbose:
            print(f"--- {turn.agent.title()} (Round {turn.round}) ---", flush=True)
            print(f"Position: {turn.parsed.position}", flush=True)
            if turn.parsed.concerns:
                print(f"Concerns: {turn.parsed.concerns}", flush=True)
            print(turn.content, flush=True)
            print(flush=True)

    def _log_vote(self, vote: Vote) -> None:
        if self.verbose:
            print(f"--- {vote.agent.title()}: {vote.vote.upper()} ---", flush=True)
            print(f"Reason: {vote.reason}", flush=True)
            print(flush=True)
