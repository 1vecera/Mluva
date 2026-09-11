"""One readable local override per prompt, with lossless legacy fallbacks and conflict checks."""

import os
import tempfile
from dataclasses import dataclass
from pathlib import Path

from mluva_linux.prompt_defaults import CLEANUP, GRILLING, NOTE, QUICK_POLISH, STRUCTURED_NOTE, TASK_SPEC, TITLE

MAX_PROMPT_CHARACTERS = 8000


@dataclass(frozen=True)
class Prompt:
    """Give editable task text a stable file identity independent of its display name."""

    identifier: str
    name: str
    purpose: str
    default: str
    built_in: bool = True


BUILT_INS = (
    Prompt(
        "cleanup",
        "Dictation cleanup",
        "Optional faithful cleanup; fact and token integrity checks remain fixed.",
        CLEANUP,
    ),
    Prompt("live-grilling", "Live · Grilling", "Develop an idea and surface the next unanswered questions.", GRILLING),
    Prompt(
        "live-task-spec",
        "Live · Task spec",
        "Fill the separately editable task structure.",
        "Fill the task specification template from the speaker's words.",
    ),
    Prompt(
        "live-structured-note",
        "Live · Structured note",
        "Organize dictation using the note structure.",
        STRUCTURED_NOTE + " Keep sections that identify missing information.",
    ),
    Prompt("live-polish", "Live · Polish", "Polish a growing draft during dictation.", QUICK_POLISH),
    Prompt("live-custom", "Live · Custom", "Your own instructions for live dictation.", ""),
    Prompt(
        "template-task-spec",
        "Template · Task spec structure",
        "Initial Markdown structure for Live Task spec.",
        TASK_SPEC,
    ),
    Prompt(
        "template-structured-note",
        "Template · Note structure",
        "Initial Markdown structure for Live Structured note.",
        NOTE,
    ),
    Prompt("rewrite-polish", "Rewrite · Polish", "Polish the current conversation on request.", QUICK_POLISH),
    Prompt(
        "rewrite-structure",
        "Rewrite · Structure",
        "Turn the current conversation into a structured note.",
        STRUCTURED_NOTE,
    ),
    Prompt("title", "Conversation title", "Name a completed conversation when automatic titles are enabled.", TITLE),
)


@dataclass(frozen=True)
class PromptState:
    """Keep raw local bytes as an optimistic concurrency token, never as provider input."""

    text: str
    token: bytes | None
    overridden: bool
    error: str = ""


class PromptStore:
    """Resolve files before legacy/default text; invalid files stay intact and visible in the editor."""

    def __init__(self, directory: Path, custom_live: str = "", styles=()) -> None:
        """Register defaults and retained legacy baselines without modifying user files."""
        self.directory = directory
        self.catalog = {prompt.identifier: prompt for prompt in BUILT_INS}
        if custom_live:
            prompt = self.catalog["live-custom"]
            self.catalog[prompt.identifier] = Prompt(prompt.identifier, prompt.name, prompt.purpose, custom_live, False)
        self.sync_styles(styles)

    def sync_styles(self, styles) -> None:
        """Use original saved text as a recovery baseline, retaining UUID-based selections."""
        self.catalog = {key: value for key, value in self.catalog.items() if not key.startswith("style-")}
        for style in styles:
            identifier = "style-" + style.identifier.lower()
            self.catalog[identifier] = Prompt(
                identifier,
                "Style · " + style.name,
                "Saved rewrite and capture output style.",
                style.instructions,
                style.is_built_in,
            )

    def path(self, identifier: str) -> Path:
        """Accept only catalog identities, never paths supplied by a prompt."""
        if identifier not in self.catalog:
            raise KeyError(identifier)
        return self.directory / (identifier + ".md")

    def read(self, identifier: str) -> PromptState:
        """Recover invalid overrides without deleting them or hiding the effective fallback."""
        prompt = self.catalog[identifier]
        path = self.path(identifier)
        try:
            raw = path.read_bytes()
        except FileNotFoundError:
            return PromptState(prompt.default, None, False)
        except OSError as error:
            return PromptState(
                prompt.default, None, True, f"Cannot read {path.name}: {error.strerror}. Using the baseline."
            )
        try:
            text = raw.decode("utf-8")
            self.validate(identifier, text)
            return PromptState(text, raw, True)
        except (UnicodeError, ValueError) as error:
            return PromptState(prompt.default, raw, True, f"{path.name}: {error}. Using the baseline until repaired.")

    def snapshot(self) -> dict[str, str]:
        """Freeze effective text for one request or an entire Live session."""
        return {identifier: self.read(identifier).text for identifier in self.catalog}

    def validate(self, identifier: str, text: str) -> None:
        """Bound editable instructions without interpreting Markdown or template braces."""
        self.path(identifier)
        if len(text) > MAX_PROMPT_CHARACTERS:
            raise ValueError(f"Use at most {MAX_PROMPT_CHARACTERS:,} characters")
        if not text.strip() and identifier != "live-custom":
            raise ValueError("The prompt cannot be empty")
        if any(ord(char) < 32 and char not in "\n\r\t" for char in text):
            raise ValueError("Remove control characters")

    def save(self, identifier: str, text: str, expected: bytes | None) -> None:
        """Atomically replace exactly one reviewed file, refusing a concurrent local edit."""
        self.validate(identifier, text)
        self._check_conflict(identifier, expected)
        self.directory.mkdir(mode=0o700, parents=True, exist_ok=True)
        descriptor, temporary = tempfile.mkstemp(prefix=".prompt-", dir=self.directory)
        try:
            with os.fdopen(descriptor, "w", encoding="utf-8", newline="") as stream:
                stream.write(text)
                stream.flush()
                os.fsync(stream.fileno())
            os.replace(temporary, self.path(identifier))
        finally:
            Path(temporary).unlink(missing_ok=True)

    def reset(self, identifier: str, expected: bytes | None) -> None:
        """Remove only the selected override; legacy custom text remains recoverable."""
        self._check_conflict(identifier, expected)
        self.path(identifier).unlink(missing_ok=True)

    def _check_conflict(self, identifier: str, expected: bytes | None) -> None:
        path = self.path(identifier)
        try:
            actual = path.read_bytes()
        except FileNotFoundError:
            actual = None
        if actual != expected:
            raise ValueError(
                "This file changed locally. Your draft is kept. Copy it, then cancel and reopen to reload."
            )
