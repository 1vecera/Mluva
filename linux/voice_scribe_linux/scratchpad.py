"""Persistent recovery state for one unresolved Linux Scratchpad draft."""

import json
from dataclasses import asdict, dataclass, field
from pathlib import Path


@dataclass(frozen=True, slots=True)
class ScratchpadDraft:
    """Preserve editable text beside its raw transcript and retained recording."""

    identifier: str
    history_identifier: str | None
    created_at: str
    raw_text: str
    text: str
    audio_path: str | None
    incognito: bool = False
    audio_retention_policy: str = "failures"
    session_identifier: str | None = None


@dataclass(slots=True)
class ScratchpadDraftStore:
    """Persist at most one unresolved draft with owner-only permissions."""

    path: Path
    draft: ScratchpadDraft | None = field(init=False)
    persistence_error: str | None = field(init=False, default=None)

    def __post_init__(self) -> None:
        """Restore unresolved work immediately when the application constructs the store."""
        self.draft = self._load()

    def save(self, draft: ScratchpadDraft, persist: bool = True) -> None:
        """Keep a draft in memory and atomically persist it only when privacy permits."""
        if self.persistence_error is not None:
            raise RuntimeError(
                "Scratchpad changes are disabled until the malformed recovery document is repaired: "
                f"{self.persistence_error}"
            )
        if draft.incognito:
            persist = False
        if not persist:
            self.path.unlink(missing_ok=True)
            self.path.with_suffix(".tmp").unlink(missing_ok=True)
            self.draft = draft
            return
        self.path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        temporary_path = self.path.with_suffix(".tmp")
        try:
            temporary_path.write_text(json.dumps(asdict(draft), indent=2) + "\n", encoding="utf-8")
            temporary_path.chmod(0o600)
            temporary_path.replace(self.path)
        finally:
            temporary_path.unlink(missing_ok=True)
        self.draft = draft

    def clear(self, remove_audio: bool) -> None:
        """Resolve the active draft and optionally erase its retained recording."""
        if remove_audio and self.draft is not None and self.draft.audio_path is not None:
            audio_path = Path(self.draft.audio_path).resolve()
            recordings_directory = (self.path.parent / "recordings").resolve()
            if not audio_path.is_relative_to(recordings_directory):
                raise ValueError(f"Refusing to delete Scratchpad audio outside {recordings_directory}")
            audio_path.unlink(missing_ok=True)
        self.path.unlink(missing_ok=True)
        self.path.with_suffix(".tmp").unlink(missing_ok=True)
        self.draft = None

    def _load(self) -> ScratchpadDraft | None:
        """Return persisted work when present without inventing an empty draft."""
        if not self.path.exists():
            return None
        try:
            payload = json.loads(self.path.read_text(encoding="utf-8"))
            if not isinstance(payload, dict):
                raise ValueError("Scratchpad recovery document must be a JSON object.")
            payload.setdefault("incognito", False)
            payload.setdefault("audio_retention_policy", "failures")
            payload.setdefault("session_identifier", None)
            return ScratchpadDraft(**payload)
        except (OSError, TypeError, ValueError) as error:
            self.persistence_error = str(error)
            return None
