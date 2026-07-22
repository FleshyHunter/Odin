import re
from enum import Enum

from app.generation.service import generate_text


class Intent(str, Enum):
    DEFINITION = "DEFINITION"
    EXPLANATION = "EXPLANATION"
    EXERCISE = "EXERCISE"
    CLARIFICATION = "CLARIFICATION"
    HINT = "HINT"
    RETRY_EXERCISE = "RETRY_EXERCISE"
    TANGENT = "TANGENT"
    OUT_OF_SCOPE = "OUT_OF_SCOPE"


# Ordered — first match wins. Cheap, ~0ms, covers the bulk of real
# traffic (ARCHITECTURE.md: "rules first, qwen fallback only on no
# match").
_RULES: list[tuple[re.Pattern, Intent]] = [
    (re.compile(r"^\s*(what\s+is|what's|define)\b", re.IGNORECASE), Intent.DEFINITION),
    (re.compile(r"\b(retry|try again|redo)\b", re.IGNORECASE), Intent.RETRY_EXERCISE),
    (re.compile(r"\b(quiz me|practice|give me an exercise|test me)\b", re.IGNORECASE), Intent.EXERCISE),
    (re.compile(r"\bhint\b", re.IGNORECASE), Intent.HINT),
    (re.compile(r"\b(i don'?t understand|i'?m confused|confusing|lost)\b", re.IGNORECASE), Intent.CLARIFICATION),
    (re.compile(r"\bexplain\b", re.IGNORECASE), Intent.EXPLANATION),
]

_FALLBACK_CATEGORIES = [
    Intent.DEFINITION,
    Intent.EXPLANATION,
    Intent.EXERCISE,
    Intent.CLARIFICATION,
    Intent.HINT,
    Intent.RETRY_EXERCISE,
]

_FALLBACK_PROMPT = """Classify the student's message into exactly one category: {categories}.

Message: "{text}"

Reply with only the category name, nothing else."""


def _classify_by_rules(text: str) -> Intent | None:
    for pattern, intent in _RULES:
        if pattern.search(text):
            return intent
    return None


def _classify_by_model(text: str) -> Intent:
    categories = ", ".join(i.value for i in _FALLBACK_CATEGORIES)
    prompt = _FALLBACK_PROMPT.format(categories=categories, text=text)
    # think=False: this is a short classification call, not the deep
    # reasoning generate_text's think=True default exists for — see
    # generation/service.py's own reasoning for that default.
    response = generate_text(prompt, think=False)
    cleaned = response.strip().upper()
    for intent in _FALLBACK_CATEGORIES:
        if intent.value == cleaned:
            return intent
    # Fail-open default when the model doesn't return a clean label —
    # same convention as Block 12's exercise-template validation.
    return Intent.EXPLANATION


def classify_intent(text: str, is_on_topic: bool, current_concept_id: str | None = None) -> Intent:
    """Step 6 — two-step intent classification: rules first (~0ms),
    qwen fallback only on no rule match (ARCHITECTURE.md).

    current_concept_id is the missing half of the locked "off-topic
    mid-journey -> TANGENT" rule (PRD.md, Tangent Mode): is_on_topic
    already detects off-topic; this says whether the student is
    mid-journey (current_concept_id present) or not (absent/None).
    ai_service never validates this as a real UUID or checks it against
    anything — it has no DB access — it's used purely as a presence
    flag for this one branch.

    is_on_topic gates BEFORE rules run, not after — OUT_OF_SCOPE/TANGENT
    are high-stakes categories (ARCHITECTURE.md: misrouting touches
    journey/DAG state), so a syntactic match like "what is X" must not
    override them. Caught live: "what is the capital of France" against
    a math vocabulary matched the DEFINITION rule before this fix,
    which is exactly the wrong call for an off-topic question.
    """
    if not is_on_topic:
        return Intent.TANGENT if current_concept_id is not None else Intent.OUT_OF_SCOPE
    rule_match = _classify_by_rules(text)
    if rule_match is not None:
        return rule_match
    return _classify_by_model(text)
