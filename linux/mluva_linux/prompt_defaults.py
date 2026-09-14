"""Editable task defaults; transport and source-integrity rules remain in their callers."""

from dataclasses import dataclass

QUICK_POLISH = (
    "Polish the text faithfully. Remove filler words, false starts and accidental repetitions; "
    "fix grammar and punctuation and lightly paraphrase for clarity. Keep the speaker's language, "
    "voice, meaning, facts, names, numbers and technical terms. Do not summarize or invent details."
)
STRUCTURED_NOTE = (
    "Turn the text into a structured note in the same language. Lead with a concise summary, "
    "then group the details into clear bullet points, most important first. Preserve facts, names, "
    "numbers, uncertainty and technical terms. Do not invent decisions, owners or deadlines."
)
TASK_SPEC = """# Task
[Missing: task name]

## Intent
[Missing: outcome and why it matters]

## Requirements
[Missing: what to build or change]

## Constraints
[Missing: boundaries, technologies and things to preserve]

## Success criteria
[Missing: observable checks]

## Open questions
[Missing: unresolved decisions or approvals]"""
NOTE = """# Note
[Missing: subject]

## Summary
[Missing: main point]

## Details
[Missing: supporting information]

## Decisions and next steps
[Missing: explicit decisions or next steps]

## Open questions
[Missing: what still needs clarification]"""
INITIAL_MINIMUM_CHARACTERS = 40
GRILLING = (
    "Help the speaker develop and stress-test their idea without interrupting their speech. "
    "Put ## Questions first and ## Architecture after it. Omit either section until it has content. "
    "Under Questions keep one to three short, specific unanswered questions, most consequential first. "
    "Ask only questions whose prerequisites are already answered; do not assume unresolved decisions. "
    "On every update, remove questions answered anywhere in the transcript, incorporate those answers below, "
    "and advance to the next unresolved decisions. Do not repeat answered questions or demand an answer "
    "before making progress. When no material questions remain, omit the Questions section. "
    "Under Architecture build a concise working design in the speaker's language. Consider intent, "
    "requirements, constraints, preferences, technologies, components, interfaces, decisions and success criteria. "
    "Only create a field or heading when speech supplies useful content for it; hide empty fields and templates. "
    "Do not fill the page with missing-information placeholders. Explicit unknowns belong in Questions. "
    "Include a small fenced mermaid flowchart or sequenceDiagram when the speaker has described relationships "
    "that a sketch makes clearer. Use only supplied components and connections, no invented architecture. "
    "Use plain labels and standard Mermaid syntax, without links, HTML, directives or frontmatter. "
    "Keep Questions and Architecture as these exact headings; the content follows the speaker's language."
)

TITLE = "Write a specific 3–7 word title in the original language. Name the subject, not the act of dictating."

CLEANUP = "Faithfully clean this dictated text. Remove obvious filler and repair punctuation only."


@dataclass(frozen=True, slots=True)
class DraftTemplate:
    """Describe an optional editable starting structure for a Live mode."""

    name: str
    purpose: str
    text: str


@dataclass(frozen=True, slots=True)
class LiveTemplate:
    """Define one Live mode for validation, menus, prompt editing and execution."""

    identifier: str
    name: str
    purpose: str
    instructions: str
    draft: DraftTemplate | None = None


LIVE_TEMPLATES = (
    LiveTemplate("grilling", "Grilling", "Develop an idea and surface the next unanswered questions.", GRILLING),
    LiveTemplate(
        "task-spec",
        "Task spec",
        "Fill the separately editable task structure.",
        "Fill the task specification template from the speaker's words.",
        DraftTemplate("Task spec structure", "Initial Markdown structure for Live Task spec.", TASK_SPEC),
    ),
    LiveTemplate(
        "structured-note",
        "Structured note",
        "Organize dictation using the note structure.",
        STRUCTURED_NOTE + " Keep sections that identify missing information.",
        DraftTemplate("Note structure", "Initial Markdown structure for Live Structured note.", NOTE),
    ),
    LiveTemplate("polish", "Polish", "Polish a growing draft during dictation.", QUICK_POLISH),
    LiveTemplate("custom", "Custom", "Your own instructions for live dictation.", ""),
)
LIVE_TEMPLATE_BY_ID = {template.identifier: template for template in LIVE_TEMPLATES}
TEMPLATE_CHOICES = tuple((template.identifier, template.name) for template in LIVE_TEMPLATES)
