from dataclasses import dataclass

import sympy

from app.grading.safe_parse import UnsafeExpressionError, safe_parse_symbolic
from app.nlp.spacy_pipe import get_nlp

REFORMAT_FEEDBACK = "Please reformat your answer using standard mathematical notation."


@dataclass
class GradeResult:
    is_correct: bool
    score: float
    feedback: str | None = None


def grade_mcq(student_answer: str, correct_answer: str) -> GradeResult:
    """PRD.md, Assessment Engine: normalize (lowercase, strip) -> exact match."""
    is_correct = student_answer.strip().lower() == correct_answer.strip().lower()
    return GradeResult(is_correct=is_correct, score=1.0 if is_correct else 0.0)


def grade_numeric(student_answer: str, correct_answer: str, tolerance: float) -> GradeResult:
    """PRD.md: parse float -> abs(student - correct) <= tolerance."""
    try:
        student_value = float(student_answer.strip())
        correct_value = float(correct_answer.strip())
    except ValueError:
        return GradeResult(is_correct=False, score=0.0, feedback="Please enter a number.")

    is_correct = abs(student_value - correct_value) <= tolerance
    return GradeResult(is_correct=is_correct, score=1.0 if is_correct else 0.0)


def grade_symbolic(student_answer: str, correct_answer: str) -> GradeResult:
    """PRD.md: SymPy — normalize ("2x"->"2*x" etc, handled by asteval's
    own operand evaluation, not a separate string-rewrite step),
    simplify(s - c) == 0, catch parse failure -> ask to reformat.

    student_answer is untrusted and goes through safe_parse_symbolic
    (asteval AST-allowlist), never SymPy's own parse_expr/sympify
    directly — see safe_parse.py for why. correct_answer is trusted
    (Claude-authored exercise content), safe to sympify() directly.
    """
    try:
        correct_expr = sympy.sympify(correct_answer)
    except (sympy.SympifyError, TypeError, ValueError):
        # A malformed correct_answer is a content bug, not a student
        # error, but from this API's perspective it's still "can't
        # grade this attempt."
        return GradeResult(is_correct=False, score=0.0, feedback="This exercise could not be graded — please report it.")

    try:
        student_expr = safe_parse_symbolic(student_answer, correct_expr)
    except UnsafeExpressionError:
        return GradeResult(is_correct=False, score=0.0, feedback=REFORMAT_FEEDBACK)

    try:
        diff = sympy.simplify(student_expr - correct_expr)
    except Exception:
        return GradeResult(is_correct=False, score=0.0, feedback=REFORMAT_FEEDBACK)

    is_correct = diff == 0
    return GradeResult(is_correct=is_correct, score=1.0 if is_correct else 0.0)


def _normalize_fill_blank(text: str) -> str:
    doc = get_nlp()(text.lower())
    return " ".join(tok.lemma_ for tok in doc if tok.is_alpha)


def grade_fill_blank(student_answer: str, accepted_answers: list[str]) -> GradeResult:
    """PRD.md: normalize (lowercase, lemmatize, strip punct) -> match
    accepted set. Reuses nlp/spacy_pipe.py's cached model (Step 5's
    infra) rather than a second lemmatizer — same reuse pattern as
    Block 8's fuzzy_match.py.
    """
    normalized_student = _normalize_fill_blank(student_answer)
    normalized_accepted = {_normalize_fill_blank(a) for a in accepted_answers}
    is_correct = normalized_student in normalized_accepted
    return GradeResult(is_correct=is_correct, score=1.0 if is_correct else 0.0)


def grade_short_answer(
    student_answer: str,
    required_keywords: list[str],
    accepted_synonyms: dict[str, list[str]] | None,
    threshold: float,
) -> GradeResult:
    """PRD.md: keyword scoring, score = found/required, >= 0.6 correct."""
    if not required_keywords:
        return GradeResult(is_correct=False, score=0.0)

    accepted_synonyms = accepted_synonyms or {}
    normalized_student = student_answer.lower()
    found = 0
    for keyword in required_keywords:
        variants = [keyword, *accepted_synonyms.get(keyword, [])]
        if any(variant.lower() in normalized_student for variant in variants):
            found += 1

    score = found / len(required_keywords)
    return GradeResult(is_correct=score >= threshold, score=score)


def grade_exercise(
    exercise_type: str,
    student_answer: str,
    correct_answer: str | None,
    tolerance: float | None,
    grader_config: dict | None,
) -> GradeResult:
    """Thin dispatcher — all 5 grader types from PRD.md's Assessment
    Engine, routed by exercise_type (matches exercises.exercise_type's
    CHECK constraint values exactly, SCHEMA.md). No LLM call anywhere
    in this path (Rule 2/Rule 50) — every grader below is deterministic.
    """
    grader_config = grader_config or {}

    if exercise_type == "mcq":
        return grade_mcq(student_answer, correct_answer or "")
    if exercise_type == "numeric":
        return grade_numeric(student_answer, correct_answer or "", tolerance if tolerance is not None else 0.0)
    if exercise_type == "symbolic_math":
        return grade_symbolic(student_answer, correct_answer or "")
    if exercise_type == "fill_blank":
        return grade_fill_blank(student_answer, grader_config.get("accepted_answers", []))
    if exercise_type == "short_answer":
        return grade_short_answer(
            student_answer,
            grader_config.get("required_keywords", []),
            grader_config.get("accepted_synonyms"),
            grader_config.get("threshold", 0.6),
        )

    raise ValueError(f"unknown exercise_type: {exercise_type!r}")
