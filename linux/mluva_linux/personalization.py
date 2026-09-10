"""Deterministic transcript personalization and owner-only local persistence."""

from __future__ import annotations

import json
import os
import re
import uuid
from dataclasses import dataclass, field
from datetime import datetime
from enum import StrEnum
from pathlib import Path

MAX_APPLICATION_IDENTIFIER_CHARACTERS = 2_048
MAX_CUSTOM_STYLES = 100
MAX_DICTIONARY_ENTRIES = 1_000
MAX_DICTIONARY_PHRASE_CHARACTERS = 200
MAX_DICTIONARY_REPLACEMENT_CHARACTERS = 2_000
MAX_SNIPPET_EXPANSION_CHARACTERS = 20_000
MAX_SNIPPET_TRIGGER_CHARACTERS = 200
MAX_SNIPPETS = 500
MAX_STYLE_INSTRUCTION_CHARACTERS = 8_000
MAX_STYLE_NAME_CHARACTERS = 100
PERSONALIZATION_SCHEMA_VERSION = 1
SUPPORTED_APPLICATION_MODES = frozenset(("dictation", "command", "scratchpad"))


class DictionaryCaseBehavior(StrEnum):
    """Choose whether a dictionary replacement has fixed or matched capitalization."""

    FIXED = "fixed"
    MATCH_SPOKEN = "matchSpoken"


@dataclass(frozen=True, slots=True)
class DictionaryReplacement:
    """Map one exact spoken phrase to its intended written form."""

    identifier: str
    spoken: str
    written: str
    application_identifier: str | None = None
    case_behavior: DictionaryCaseBehavior = DictionaryCaseBehavior.FIXED


@dataclass(frozen=True, slots=True)
class Snippet:
    """Expand one explicit spoken or exact typed trigger."""

    identifier: str
    trigger: str
    expansion: str
    typed_trigger: str | None = None
    application_identifier: str | None = None


@dataclass(frozen=True, slots=True)
class SavedStyle:
    """Describe one bounded Codex output-mode instruction."""

    identifier: str
    name: str
    instructions: str
    is_built_in: bool = False


BUILT_IN_STYLES = (
    SavedStyle(
        identifier="FDD26FEF-80E7-4AD3-9D08-7874266A12E8",
        name="Message",
        instructions=(
            "Rewrite as a concise, natural chat message. Preserve every fact, request, technical term, and level "
            "of certainty."
        ),
        is_built_in=True,
    ),
    SavedStyle(
        identifier="5DD73A99-80C2-4FA9-99C2-DC7528300248",
        name="Google Chat",
        instructions=(
            "Rewrite as a concise, natural Google Chat message in the speaker's language. Lead with the point or "
            "request, use short paragraphs, and use bullets only when they improve scanability. Preserve every "
            "fact, constraint, name, and level of certainty. Do not invent a greeting, recipient, emoji, or sign-off."
        ),
        is_built_in=True,
    ),
    SavedStyle(
        identifier="7AD20551-3DC5-4774-9D34-DB4704F3A05F",
        name="Tasks",
        instructions=(
            "Rewrite as task-ready text with a short action-oriented title followed by compact bullets for the "
            "outcome, context, and constraints that were actually dictated. Preserve explicit owners and dates. "
            "Never invent an assignee, deadline, priority, or acceptance criterion."
        ),
        is_built_in=True,
    ),
    SavedStyle(
        identifier="0E5FB9A9-56CF-4C6F-A825-26D4057AB5AB",
        name="Email",
        instructions=(
            "Rewrite as a clear professional email body. Preserve every fact and request. Do not invent a greeting, "
            "recipient, or sign-off."
        ),
        is_built_in=True,
    ),
    SavedStyle(
        identifier="58D7C7D2-DDBA-41C8-B6B8-497220E9D5A9",
        name="Prose",
        instructions=(
            "Rewrite as polished connected prose with natural paragraphs. Preserve meaning, facts, and the speaker's "
            "level of certainty."
        ),
        is_built_in=True,
    ),
    SavedStyle(
        identifier="84D3302E-8886-4079-88C1-A86F1B4BE7D6",
        name="Technical notes",
        instructions=(
            "Rewrite as compact structured technical notes. Preserve commands, identifiers, paths, URLs, versions, "
            "numbers, and negation exactly."
        ),
        is_built_in=True,
    ),
    SavedStyle(
        identifier="54680987-230B-4E27-851C-AAD358AA54B3",
        name="Prompt",
        instructions=(
            "Rewrite as a clear reusable instruction for an AI system. Preserve constraints and source facts. Do not "
            "answer the instruction."
        ),
        is_built_in=True,
    ),
)

_BUILT_IN_STYLE_IDS = frozenset(style.identifier.casefold() for style in BUILT_IN_STYLES)
_BUILT_IN_STYLE_NAMES = frozenset(style.name.casefold() for style in BUILT_IN_STYLES)
_SNIPPET_VARIABLE_PATTERN = re.compile(r"\{\{([A-Za-z][A-Za-z0-9_]*)\}\}")
_PROTECTED_TOKEN_PATTERN = re.compile(
    r"(?i:https?)://[^\s<>()]+"
    r"|(?<![\w])/[A-Za-z0-9._~-]+(?:/[A-Za-z0-9._~-]+)+"
    r"|`[^`\n]+`"
    r"|--[A-Za-z0-9][A-Za-z0-9-]*"
    r"|(?i:\b(?:do\s+not|does\s+not|did\s+not|not|never|without|cannot|can't|won't|don't)\b)"
    r"|\b[A-Za-z][A-Za-z0-9]*_[A-Za-z0-9_]+\b"
    r"|\b[a-z]+(?:[A-Z][A-Za-z0-9]*)+\b"
    r"|\b\d+(?:\.\d+)?%"
    r"|\b(?=[A-Za-z0-9-]*\d)[A-Za-z]+(?:-[A-Za-z0-9]+)+\b"
    r"|\b\d+(?:\.\d+)*\b"
)


@dataclass(slots=True)
class PersonalizationStore:
    """Persist dictionaries, snippets, styles, and local application selections."""

    path: Path
    dictionary: list[DictionaryReplacement] = field(init=False, default_factory=list)
    snippets: list[Snippet] = field(init=False, default_factory=list)
    custom_styles: list[SavedStyle] = field(init=False, default_factory=list)
    default_style_identifier: str | None = field(init=False, default=None)
    application_style_identifiers: dict[str, str] = field(init=False, default_factory=dict)
    application_style_disabled_identifiers: set[str] = field(init=False, default_factory=set)
    application_modes: dict[str, str] = field(init=False, default_factory=dict)
    application_provider_preferences: dict[str, str] = field(init=False, default_factory=dict)
    dismissed_vocabulary_suggestion_identifiers: set[str] = field(init=False, default_factory=set)
    persistence_error: str | None = field(init=False, default=None)

    def __post_init__(self) -> None:
        """Load a compatible document without replacing malformed user data."""
        self._load()

    @property
    def styles(self) -> tuple[SavedStyle, ...]:
        """Return immutable built-ins followed by durable custom styles."""
        return BUILT_IN_STYLES + tuple(self.custom_styles)

    @property
    def recognition_context(self) -> tuple[str, ...]:
        """Return unique dictionary and spoken-trigger phrases for provider hints and validation."""
        candidates = [value for replacement in self.dictionary for value in (replacement.spoken, replacement.written)]
        candidates.extend(snippet.trigger for snippet in self.snippets)
        seen: set[str] = set()
        unique: list[str] = []
        for candidate in candidates:
            normalized = _normalized_phrase(candidate)
            if normalized not in seen:
                seen.add(normalized)
                unique.append(candidate)
        return tuple(unique)

    def scoped_recognition_context(self, application_identifier: str | None) -> tuple[str, ...]:
        """Return only vocabulary eligible for one local application scope."""
        dictionary = self.scoped_dictionary(application_identifier)
        snippets = self.scoped_snippets(application_identifier)
        candidates = [value for replacement in dictionary for value in (replacement.spoken, replacement.written)]
        candidates.extend(snippet.trigger for snippet in snippets)
        seen: set[str] = set()
        unique: list[str] = []
        for candidate in candidates:
            normalized = _normalized_phrase(candidate)
            if normalized not in seen:
                seen.add(normalized)
                unique.append(candidate)
        return tuple(unique)

    def save_dictionary_replacement(
        self,
        spoken: str,
        written: str,
        application_identifier: str | None = None,
        case_behavior: DictionaryCaseBehavior = DictionaryCaseBehavior.FIXED,
    ) -> DictionaryReplacement:
        """Create or update one scoped phrase and persist the complete document atomically."""
        spoken = _bounded_required_text(spoken, "Spoken phrase", MAX_DICTIONARY_PHRASE_CHARACTERS)
        written = _bounded_required_text(written, "Written replacement", MAX_DICTIONARY_REPLACEMENT_CHARACTERS)
        application_identifier = _normalized_application_identifier(application_identifier)
        if not isinstance(case_behavior, DictionaryCaseBehavior):
            raise TypeError("case_behavior must be a DictionaryCaseBehavior")
        index = next(
            (
                position
                for position, replacement in enumerate(self.dictionary)
                if _normalized_phrase(replacement.spoken) == _normalized_phrase(spoken)
                and replacement.application_identifier == application_identifier
            ),
            None,
        )
        if index is None:
            if len(self.dictionary) >= MAX_DICTIONARY_ENTRIES:
                raise ValueError(f"Dictionary supports at most {MAX_DICTIONARY_ENTRIES:,} entries")
            replacement = DictionaryReplacement(
                identifier=str(uuid.uuid4()),
                spoken=spoken,
                written=written,
                application_identifier=application_identifier,
                case_behavior=case_behavior,
            )
            updated = [*self.dictionary, replacement]
        else:
            replacement = DictionaryReplacement(
                identifier=self.dictionary[index].identifier,
                spoken=spoken,
                written=written,
                application_identifier=application_identifier,
                case_behavior=case_behavior,
            )
            updated = list(self.dictionary)
            updated[index] = replacement
        self._persist(dictionary=updated)
        self.dictionary = updated
        return replacement

    def delete_dictionary_replacement(self, identifier: str) -> None:
        """Delete one exact dictionary identifier without broad matching."""
        updated = [replacement for replacement in self.dictionary if replacement.identifier != identifier]
        if len(updated) == len(self.dictionary):
            raise KeyError(identifier)
        self._persist(dictionary=updated)
        self.dictionary = updated

    def save_snippet(
        self,
        trigger: str,
        expansion: str,
        typed_trigger: str | None = None,
        application_identifier: str | None = None,
    ) -> Snippet:
        """Create or update an explicit scoped snippet and optional exact typed token."""
        trigger = _bounded_required_text(trigger, "Spoken trigger", MAX_SNIPPET_TRIGGER_CHARACTERS)
        expansion = _bounded_required_text(expansion, "Snippet expansion", MAX_SNIPPET_EXPANSION_CHARACTERS)
        typed_trigger = _normalized_typed_trigger(typed_trigger)
        application_identifier = _normalized_application_identifier(application_identifier)
        index = next(
            (
                position
                for position, snippet in enumerate(self.snippets)
                if snippet.application_identifier == application_identifier
                and (
                    _normalized_phrase(snippet.trigger) == _normalized_phrase(trigger)
                    or (typed_trigger is not None and snippet.typed_trigger == typed_trigger)
                )
            ),
            None,
        )
        if index is None:
            if len(self.snippets) >= MAX_SNIPPETS:
                raise ValueError(f"Snippets support at most {MAX_SNIPPETS:,} entries")
            snippet = Snippet(
                identifier=str(uuid.uuid4()),
                trigger=trigger,
                expansion=expansion,
                typed_trigger=typed_trigger,
                application_identifier=application_identifier,
            )
            updated = [*self.snippets, snippet]
        else:
            snippet = Snippet(
                identifier=self.snippets[index].identifier,
                trigger=trigger,
                expansion=expansion,
                typed_trigger=typed_trigger,
                application_identifier=application_identifier,
            )
            updated = list(self.snippets)
            updated[index] = snippet
        self._persist(snippets=updated)
        self.snippets = updated
        return snippet

    def delete_snippet(self, identifier: str) -> None:
        """Delete one exact snippet identifier without changing other scopes."""
        updated = [snippet for snippet in self.snippets if snippet.identifier != identifier]
        if len(updated) == len(self.snippets):
            raise KeyError(identifier)
        self._persist(snippets=updated)
        self.snippets = updated

    def dismiss_vocabulary_suggestion(self, identifier: str) -> None:
        """Persist one explicit review decision without changing the dictionary."""
        if not identifier:
            raise ValueError("Vocabulary suggestion identifier cannot be empty")
        if identifier in self.dismissed_vocabulary_suggestion_identifiers:
            return
        updated = {*self.dismissed_vocabulary_suggestion_identifiers, identifier}
        self._persist(dismissed_vocabulary_suggestion_identifiers=updated)
        self.dismissed_vocabulary_suggestion_identifiers = updated

    def save_style(self, name: str, instructions: str) -> SavedStyle:
        """Create or update one custom style without shadowing a built-in name."""
        name = _bounded_required_text(name, "Style name", MAX_STYLE_NAME_CHARACTERS)
        instructions = _bounded_required_text(
            instructions,
            "Style instructions",
            MAX_STYLE_INSTRUCTION_CHARACTERS,
        )
        normalized_name = _normalized_phrase(name)
        if normalized_name in _BUILT_IN_STYLE_NAMES:
            raise ValueError("Built-in style names cannot be replaced")
        index = next(
            (
                position
                for position, style in enumerate(self.custom_styles)
                if _normalized_phrase(style.name) == normalized_name
            ),
            None,
        )
        if index is None:
            if len(self.custom_styles) >= MAX_CUSTOM_STYLES:
                raise ValueError(f"Custom styles support at most {MAX_CUSTOM_STYLES:,} entries")
            style = SavedStyle(str(uuid.uuid4()), name, instructions)
            updated = [*self.custom_styles, style]
        else:
            style = SavedStyle(self.custom_styles[index].identifier, name, instructions)
            updated = list(self.custom_styles)
            updated[index] = style
        self._persist(custom_styles=updated)
        self.custom_styles = updated
        return style

    def update_style(self, identifier: str, name: str, instructions: str) -> SavedStyle:
        """Edit one custom style by identifier while keeping built-ins immutable."""
        current = self.style(identifier)
        if current is None or current.is_built_in:
            raise KeyError(identifier)
        name = _bounded_required_text(name, "Style name", MAX_STYLE_NAME_CHARACTERS)
        instructions = _bounded_required_text(
            instructions,
            "Style instructions",
            MAX_STYLE_INSTRUCTION_CHARACTERS,
        )
        normalized_name = _normalized_phrase(name)
        if any(
            _normalized_phrase(style.name) == normalized_name and style.identifier.casefold() != identifier.casefold()
            for style in self.styles
        ):
            raise ValueError("Style names must be unique")
        updated_style = SavedStyle(current.identifier, name, instructions)
        updated = [updated_style if style.identifier == current.identifier else style for style in self.custom_styles]
        self._persist(custom_styles=updated)
        self.custom_styles = updated
        return updated_style

    def delete_style(self, identifier: str) -> None:
        """Delete one custom style and every selection that referenced it."""
        current = self.style(identifier)
        if current is None or current.is_built_in:
            raise KeyError(identifier)
        updated_styles = [style for style in self.custom_styles if style.identifier != current.identifier]
        default_style_identifier = (
            None if self.default_style_identifier == current.identifier else self.default_style_identifier
        )
        application_style_identifiers = {
            application: style_identifier
            for application, style_identifier in self.application_style_identifiers.items()
            if style_identifier != current.identifier
        }
        self._persist(
            custom_styles=updated_styles,
            default_style_identifier=default_style_identifier,
            application_style_identifiers=application_style_identifiers,
        )
        self.custom_styles = updated_styles
        self.default_style_identifier = default_style_identifier
        self.application_style_identifiers = application_style_identifiers

    def style(self, identifier: str | None) -> SavedStyle | None:
        """Resolve one style identifier case-insensitively."""
        if identifier is None:
            return None
        normalized_identifier = identifier.casefold()
        return next(
            (style for style in self.styles if style.identifier.casefold() == normalized_identifier),
            None,
        )

    def select_style(
        self,
        identifier: str | None,
        application_identifier: str | None,
        remember_per_application: bool,
    ) -> None:
        """Persist a validated global or application-specific output style selection."""
        style = self.style(identifier)
        validated_identifier = style.identifier if style is not None else None
        if identifier is not None and style is None:
            raise KeyError(identifier)
        application_identifier = _normalized_application_identifier(application_identifier)
        if remember_per_application and application_identifier is not None:
            updated = dict(self.application_style_identifiers)
            disabled = set(self.application_style_disabled_identifiers)
            if validated_identifier is None:
                updated.pop(application_identifier, None)
                disabled.add(application_identifier)
            else:
                updated[application_identifier] = validated_identifier
                disabled.discard(application_identifier)
            self._persist(
                application_style_identifiers=updated,
                application_style_disabled_identifiers=disabled,
            )
            self.application_style_identifiers = updated
            self.application_style_disabled_identifiers = disabled
            return
        self._persist(default_style_identifier=validated_identifier)
        self.default_style_identifier = validated_identifier

    def selected_style(
        self,
        application_identifier: str | None,
        remember_per_application: bool,
    ) -> SavedStyle | None:
        """Resolve the frozen style selection without falling across application scopes."""
        application_identifier = _normalized_application_identifier(application_identifier)
        if remember_per_application and application_identifier is not None:
            if application_identifier in self.application_style_disabled_identifiers:
                return None
            return self.style(self.application_style_identifiers.get(application_identifier))
        return self.style(self.default_style_identifier)

    def has_application_style_selection(self, application_identifier: str) -> bool:
        """Return whether an application explicitly selected a style or faithful output."""
        application_identifier = _normalized_application_identifier(application_identifier)
        if application_identifier is None:
            return False
        return (
            application_identifier in self.application_style_identifiers
            or application_identifier in self.application_style_disabled_identifiers
        )

    def select_mode(
        self,
        mode: str,
        application_identifier: str | None,
        remember_per_application: bool,
    ) -> None:
        """Remember a supported capture mode only when a concrete application is in scope."""
        if mode not in SUPPORTED_APPLICATION_MODES:
            raise ValueError(f"Unsupported capture mode: {mode}")
        application_identifier = _normalized_application_identifier(application_identifier)
        if not remember_per_application or application_identifier is None:
            return
        updated = dict(self.application_modes)
        updated[application_identifier] = mode
        self._persist(application_modes=updated)
        self.application_modes = updated

    def selected_mode(
        self,
        application_identifier: str | None,
        remember_per_application: bool,
        fallback: str,
    ) -> str:
        """Return an application profile or a caller-supplied supported fallback."""
        if fallback not in SUPPORTED_APPLICATION_MODES:
            raise ValueError(f"Unsupported capture mode: {fallback}")
        application_identifier = _normalized_application_identifier(application_identifier)
        if not remember_per_application or application_identifier is None:
            return fallback
        return self.application_modes.get(application_identifier, fallback)

    def scoped_dictionary(self, application_identifier: str | None) -> tuple[DictionaryReplacement, ...]:
        """Return global rules with same-phrase application overrides applied."""
        application_identifier = _normalized_application_identifier(application_identifier)
        if application_identifier is None:
            return tuple(replacement for replacement in self.dictionary if replacement.application_identifier is None)
        scoped = [
            replacement
            for replacement in self.dictionary
            if replacement.application_identifier == application_identifier
        ]
        scoped_keys = {_normalized_phrase(replacement.spoken) for replacement in scoped}
        global_rules = [
            replacement
            for replacement in self.dictionary
            if replacement.application_identifier is None and _normalized_phrase(replacement.spoken) not in scoped_keys
        ]
        return tuple((*global_rules, *scoped))

    def scoped_snippets(self, application_identifier: str | None) -> tuple[Snippet, ...]:
        """Return global snippets with spoken and typed application overrides applied."""
        application_identifier = _normalized_application_identifier(application_identifier)
        if application_identifier is None:
            return tuple(snippet for snippet in self.snippets if snippet.application_identifier is None)
        scoped = [snippet for snippet in self.snippets if snippet.application_identifier == application_identifier]
        scoped_spoken = {_normalized_phrase(snippet.trigger) for snippet in scoped}
        scoped_typed = {snippet.typed_trigger for snippet in scoped if snippet.typed_trigger is not None}
        global_rules = [
            snippet
            for snippet in self.snippets
            if snippet.application_identifier is None
            and _normalized_phrase(snippet.trigger) not in scoped_spoken
            and (snippet.typed_trigger is None or snippet.typed_trigger not in scoped_typed)
        ]
        return tuple((*global_rules, *scoped))

    def process_transcript(
        self,
        text: str,
        application_identifier: str | None,
        variables: dict[str, str] | None = None,
    ) -> str:
        """Apply deterministic dictionary and explicit spoken-snippet rules in contract order."""
        return personalize_transcript(
            text,
            self.scoped_dictionary(application_identifier),
            self.scoped_snippets(application_identifier),
            snippet_variables() if variables is None else variables,
        )

    def expand_typed_trigger(
        self,
        trigger: str,
        application_identifier: str | None,
        variables: dict[str, str] | None = None,
    ) -> str | None:
        """Resolve one exact typed token for a future secure desktop interception boundary."""
        matches = [
            snippet for snippet in self.scoped_snippets(application_identifier) if snippet.typed_trigger == trigger
        ]
        if not matches:
            return None
        selected = matches[-1]
        return render_snippet_expansion(
            selected.expansion,
            snippet_variables() if variables is None else variables,
        )

    def _load(self) -> None:
        """Decode one compatible macOS/Linux document while preserving malformed input."""
        if not self.path.exists():
            return
        try:
            payload = json.loads(self.path.read_text(encoding="utf-8"))
            if not isinstance(payload, dict):
                raise ValueError("Personalization document must be a JSON object")
            dictionary_payload = _object_list(payload.get("dictionary", []), "dictionary")
            snippets_payload = _object_list(payload.get("snippets", []), "snippets")
            styles_payload = _object_list(payload.get("styles", []), "styles")
            self.dictionary = [_decode_dictionary_replacement(item) for item in dictionary_payload]
            self.snippets = [_decode_snippet(item) for item in snippets_payload]
            self.custom_styles = [style for item in styles_payload if (style := _decode_custom_style(item)) is not None]
            if len(self.dictionary) > MAX_DICTIONARY_ENTRIES:
                raise ValueError("Personalization dictionary exceeds its supported bound")
            if len(self.snippets) > MAX_SNIPPETS:
                raise ValueError("Personalization snippets exceed their supported bound")
            if len(self.custom_styles) > MAX_CUSTOM_STYLES:
                raise ValueError("Personalization styles exceed their supported bound")
            available_style_ids = {style.identifier.casefold(): style.identifier for style in self.styles}
            default_style_identifier = _optional_identifier(payload.get("defaultStyleID"))
            self.default_style_identifier = available_style_ids.get(
                default_style_identifier.casefold() if default_style_identifier is not None else ""
            )
            self.application_style_identifiers = _decode_application_style_identifiers(
                payload.get("applicationStyleIDs"),
                available_style_ids,
            )
            disabled_styles = payload.get("applicationStyleDisabled", [])
            if not isinstance(disabled_styles, list) or not all(isinstance(item, str) for item in disabled_styles):
                raise ValueError("applicationStyleDisabled must be an array of strings")
            self.application_style_disabled_identifiers = {
                application_identifier
                for item in disabled_styles
                if (application_identifier := _normalized_application_identifier(item)) is not None
            }
            self.application_modes = _decode_application_modes(payload.get("applicationModes"))
            self.application_provider_preferences = _string_mapping(
                payload.get("applicationProviderPreferences"),
                "applicationProviderPreferences",
            )
            dismissed = payload.get("dismissedVocabularySuggestionIDs", [])
            if not isinstance(dismissed, list) or not all(isinstance(item, str) for item in dismissed):
                raise ValueError("dismissedVocabularySuggestionIDs must be an array of strings")
            self.dismissed_vocabulary_suggestion_identifiers = set(dismissed)
        except (OSError, ValueError, TypeError, json.JSONDecodeError) as error:
            self.dictionary = []
            self.snippets = []
            self.custom_styles = []
            self.default_style_identifier = None
            self.application_style_identifiers = {}
            self.application_style_disabled_identifiers = set()
            self.application_modes = {}
            self.application_provider_preferences = {}
            self.dismissed_vocabulary_suggestion_identifiers = set()
            self.persistence_error = str(error)

    def _persist(
        self,
        *,
        dictionary: list[DictionaryReplacement] | None = None,
        snippets: list[Snippet] | None = None,
        custom_styles: list[SavedStyle] | None = None,
        default_style_identifier: str | None | object = ...,
        application_style_identifiers: dict[str, str] | None = None,
        application_style_disabled_identifiers: set[str] | None = None,
        application_modes: dict[str, str] | None = None,
        dismissed_vocabulary_suggestion_identifiers: set[str] | None = None,
    ) -> None:
        """Atomically write one owner-only document before publishing in-memory mutations."""
        if self.persistence_error is not None:
            raise RuntimeError(
                "Personalization changes are disabled until the malformed document is repaired: "
                f"{self.persistence_error}"
            )
        persisted_default_style_identifier = (
            self.default_style_identifier if default_style_identifier is ... else default_style_identifier
        )
        payload = {
            "schemaVersion": PERSONALIZATION_SCHEMA_VERSION,
            "dictionary": [
                _encode_dictionary_replacement(item) for item in (self.dictionary if dictionary is None else dictionary)
            ],
            "snippets": [_encode_snippet(item) for item in (self.snippets if snippets is None else snippets)],
            "styles": [
                _encode_style(item) for item in (self.custom_styles if custom_styles is None else custom_styles)
            ],
            "defaultStyleID": persisted_default_style_identifier,
            "applicationStyleIDs": (
                self.application_style_identifiers
                if application_style_identifiers is None
                else application_style_identifiers
            ),
            "applicationStyleDisabled": sorted(
                self.application_style_disabled_identifiers
                if application_style_disabled_identifiers is None
                else application_style_disabled_identifiers
            ),
            "applicationModes": self.application_modes if application_modes is None else application_modes,
            "applicationProviderPreferences": self.application_provider_preferences,
            "dismissedVocabularySuggestionIDs": sorted(
                self.dismissed_vocabulary_suggestion_identifiers
                if dismissed_vocabulary_suggestion_identifiers is None
                else dismissed_vocabulary_suggestion_identifiers
            ),
        }
        self.path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        self.path.parent.chmod(0o700)
        temporary_path = self.path.with_name(f".{self.path.name}.tmp")
        try:
            temporary_path.write_text(
                json.dumps(payload, indent=2, sort_keys=True, ensure_ascii=False) + "\n",
                encoding="utf-8",
            )
            temporary_path.chmod(0o600)
            os.replace(temporary_path, self.path)
            self.path.chmod(0o600)
        except OSError:
            temporary_path.unlink(missing_ok=True)
            raise


def personalize_transcript(
    text: str,
    dictionary: tuple[DictionaryReplacement, ...],
    snippets: tuple[Snippet, ...],
    variables: dict[str, str],
) -> str:
    """Apply exact dictionary rules, then explicit spoken snippets, then whitespace cleanup."""
    result = text
    for replacement in sorted(dictionary, key=lambda item: len(item.spoken), reverse=True):
        expression = _whole_phrase_expression(replacement.spoken)
        result = expression.sub(
            lambda match, rule=replacement: _dictionary_replacement_text(rule, match.group(0)),
            result,
        )
    for snippet in sorted(snippets, key=lambda item: len(item.trigger), reverse=True):
        expression = _whole_phrase_expression(snippet.trigger, required_prefix=r"snippet\s+")
        rendered = render_snippet_expansion(snippet.expansion, variables)
        result = expression.sub(lambda _match, expansion=rendered: expansion, result)
    return _normalize_whitespace(result)


def snippet_variables(now: datetime | None = None) -> dict[str, str]:
    """Resolve documented snippet variables against the current local calendar."""
    current = datetime.now().astimezone() if now is None else now.astimezone()
    date = current.strftime("%x")
    time = current.strftime("%H:%M")
    return {
        "date": date,
        "time": time,
        "datetime": f"{date} {time}",
        "weekday": current.strftime("%A"),
    }


def render_snippet_expansion(expansion: str, variables: dict[str, str]) -> str:
    """Replace known case-insensitive variables while leaving unknown variables visible."""
    normalized = {key.casefold(): value for key, value in variables.items()}
    return _SNIPPET_VARIABLE_PATTERN.sub(
        lambda match: normalized.get(match.group(1).casefold(), match.group(0)),
        expansion,
    )


def integrity_violations(
    source: str,
    candidate: str,
    protected_vocabulary: tuple[str, ...] = (),
) -> tuple[str, ...]:
    """Return meaning-critical source tokens missing from a generative candidate."""
    protected_tokens = {match.group(0) for match in _PROTECTED_TOKEN_PATTERN.finditer(source)}
    for term in protected_vocabulary:
        if _contains_token(term, source):
            protected_tokens.add(term)
    return tuple(sorted(token for token in protected_tokens if not _contains_token(token, candidate)))


def _dictionary_replacement_text(rule: DictionaryReplacement, matched_text: str) -> str:
    """Apply the configured capitalization behavior to one exact match."""
    if rule.case_behavior is DictionaryCaseBehavior.FIXED:
        return rule.written
    letters = [character for character in matched_text if character.isalpha()]
    if not letters:
        return rule.written
    if all(character.isupper() for character in letters):
        return rule.written.upper()
    if all(character.islower() for character in letters):
        return rule.written.lower()
    if letters[0].isupper() and all(character.islower() for character in letters[1:]):
        lowered = rule.written.lower()
        return lowered[:1].upper() + lowered[1:]
    return rule.written


def _whole_phrase_expression(phrase: str, required_prefix: str = "") -> re.Pattern[str]:
    """Compile one case-insensitive Unicode word-bounded phrase expression."""
    escaped = re.escape(phrase).replace(r"\ ", r"\s+")
    return re.compile(rf"(?<!\w){required_prefix}{escaped}(?!\w)", re.IGNORECASE)


def _normalize_whitespace(text: str) -> str:
    """Collapse horizontal whitespace while preserving intentional paragraph boundaries."""
    normalized_newlines = text.replace("\r\n", "\n").replace("\r", "\n")
    lines = []
    for line in normalized_newlines.split("\n"):
        normalized_line = re.sub(r"[\t ]+", " ", line)
        normalized_line = re.sub(r"\s+([,.;:!?])", r"\1", normalized_line)
        lines.append(normalized_line.strip())
    return re.sub(r"\n{3,}", "\n\n", "\n".join(lines)).strip()


def _contains_token(token: str, text: str) -> bool:
    """Match syntax-sensitive tokens literally and natural-language terms case-insensitively."""
    case_sensitive = (
        "://" in token
        or token.startswith(("/", "--", "`"))
        or "_" in token
        or re.search(r"[a-z][A-Z]", token) is not None
    )
    return token in text if case_sensitive else token.casefold() in text.casefold()


def _bounded_required_text(value: str, label: str, maximum_characters: int) -> str:
    """Trim and validate one bounded, required user-authored string."""
    if not isinstance(value, str):
        raise TypeError(f"{label} must be text")
    normalized = value.strip()
    if not normalized:
        raise ValueError(f"{label} cannot be blank")
    if len(normalized) > maximum_characters:
        raise ValueError(f"{label} supports at most {maximum_characters:,} characters")
    return normalized


def _normalized_application_identifier(value: str | None) -> str | None:
    """Normalize one optional local-only application scope."""
    if value is None:
        return None
    return _bounded_required_text(value, "Application identifier", MAX_APPLICATION_IDENTIFIER_CHARACTERS)


def _normalized_typed_trigger(value: str | None) -> str | None:
    """Validate one optional exact, single-token typed trigger."""
    if value is None:
        return None
    if not isinstance(value, str):
        raise TypeError("Typed trigger must be text")
    if not value.strip():
        return None
    normalized = _bounded_required_text(value, "Typed trigger", MAX_SNIPPET_TRIGGER_CHARACTERS)
    if any(character.isspace() for character in normalized):
        raise ValueError("Typed triggers cannot contain whitespace")
    return normalized


def _normalized_phrase(value: str) -> str:
    """Normalize phrase identity for scoped upserts and overrides."""
    return " ".join(value.casefold().split())


def _object_list(value: object, label: str) -> list[dict[str, object]]:
    """Validate one JSON array of objects before domain decoding."""
    if not isinstance(value, list) or not all(isinstance(item, dict) for item in value):
        raise ValueError(f"{label} must be an array of objects")
    return value


def _string_mapping(value: object, label: str) -> dict[str, str]:
    """Validate one optional JSON string-to-string mapping."""
    if value is None:
        return {}
    if not isinstance(value, dict) or not all(
        isinstance(key, str) and isinstance(item, str) for key, item in value.items()
    ):
        raise ValueError(f"{label} must map strings to strings")
    return dict(value)


def _optional_identifier(value: object) -> str | None:
    """Validate and normalize one optional UUID identifier."""
    if value is None:
        return None
    if not isinstance(value, str):
        raise ValueError("Style identifiers must be strings")
    return str(uuid.UUID(value))


def _required_identifier(value: object) -> str:
    """Decode a persisted UUID or create one for legacy entries that omitted it."""
    return str(uuid.UUID(value)) if isinstance(value, str) else str(uuid.uuid4())


def _decode_dictionary_replacement(payload: dict[str, object]) -> DictionaryReplacement:
    """Decode and validate one compatible dictionary row."""
    behavior_value = payload.get("caseBehavior", DictionaryCaseBehavior.FIXED.value)
    if not isinstance(behavior_value, str):
        raise ValueError("Dictionary caseBehavior must be a string")
    return DictionaryReplacement(
        identifier=_required_identifier(payload.get("id")),
        spoken=_bounded_required_text(
            payload.get("spoken"),
            "Spoken phrase",
            MAX_DICTIONARY_PHRASE_CHARACTERS,
        ),
        written=_bounded_required_text(
            payload.get("written"),
            "Written replacement",
            MAX_DICTIONARY_REPLACEMENT_CHARACTERS,
        ),
        application_identifier=_normalized_application_identifier(payload.get("bundleIdentifier")),
        case_behavior=DictionaryCaseBehavior(behavior_value),
    )


def _decode_snippet(payload: dict[str, object]) -> Snippet:
    """Decode and validate one compatible snippet row."""
    return Snippet(
        identifier=_required_identifier(payload.get("id")),
        trigger=_bounded_required_text(
            payload.get("trigger"),
            "Spoken trigger",
            MAX_SNIPPET_TRIGGER_CHARACTERS,
        ),
        expansion=_bounded_required_text(
            payload.get("expansion"),
            "Snippet expansion",
            MAX_SNIPPET_EXPANSION_CHARACTERS,
        ),
        typed_trigger=_normalized_typed_trigger(payload.get("typedTrigger")),
        application_identifier=_normalized_application_identifier(payload.get("bundleIdentifier")),
    )


def _decode_custom_style(payload: dict[str, object]) -> SavedStyle | None:
    """Decode one custom style while refusing built-in identifier or name shadowing."""
    identifier = _required_identifier(payload.get("id"))
    name = _bounded_required_text(payload.get("name"), "Style name", MAX_STYLE_NAME_CHARACTERS)
    instructions = _bounded_required_text(
        payload.get("instructions"),
        "Style instructions",
        MAX_STYLE_INSTRUCTION_CHARACTERS,
    )
    if identifier.casefold() in _BUILT_IN_STYLE_IDS or _normalized_phrase(name) in _BUILT_IN_STYLE_NAMES:
        return None
    return SavedStyle(identifier, name, instructions)


def _decode_application_style_identifiers(
    value: object,
    available_style_identifiers: dict[str, str],
) -> dict[str, str]:
    """Discard missing style selections while preserving valid local application scopes."""
    selections = _string_mapping(value, "applicationStyleIDs")
    decoded: dict[str, str] = {}
    for application_identifier, style_identifier in selections.items():
        normalized_application = _normalized_application_identifier(application_identifier)
        try:
            normalized_style = str(uuid.UUID(style_identifier)).casefold()
        except ValueError:
            continue
        if normalized_application is not None and normalized_style in available_style_identifiers:
            decoded[normalized_application] = available_style_identifiers[normalized_style]
    return decoded


def _decode_application_modes(value: object) -> dict[str, str]:
    """Decode only supported application capture modes."""
    modes = _string_mapping(value, "applicationModes")
    return {
        application_identifier: mode
        for application_identifier, mode in modes.items()
        if mode in SUPPORTED_APPLICATION_MODES
    }


def _encode_dictionary_replacement(replacement: DictionaryReplacement) -> dict[str, object]:
    """Encode one dictionary row with the cross-platform field names."""
    return {
        "id": replacement.identifier,
        "spoken": replacement.spoken,
        "written": replacement.written,
        "bundleIdentifier": replacement.application_identifier,
        "caseBehavior": replacement.case_behavior.value,
    }


def _encode_snippet(snippet: Snippet) -> dict[str, object]:
    """Encode one snippet row with the cross-platform field names."""
    return {
        "id": snippet.identifier,
        "trigger": snippet.trigger,
        "typedTrigger": snippet.typed_trigger,
        "expansion": snippet.expansion,
        "bundleIdentifier": snippet.application_identifier,
    }


def _encode_style(style: SavedStyle) -> dict[str, object]:
    """Encode one custom style without persisting reconstructed built-in rows."""
    return {
        "id": style.identifier,
        "name": style.name,
        "instructions": style.instructions,
        "isBuiltIn": False,
    }
