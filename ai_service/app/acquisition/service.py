import json
import logging
import random

import sympy

from app.acquisition.dify_client import call_dify_workflow, parse_json_output
from app.acquisition.models import (
    Chunk,
    ConceptMeta,
    ConceptNode,
    DAGResult,
    ExerciseTemplate,
    IntakeContext,
    Resource,
)

logger = logging.getLogger(__name__)

DIFY_ACQUIRE_API_KEY_ENV = "DIFY_ACQUIRE_API_KEY"
DIFY_DAG_API_KEY_ENV = "DIFY_DAG_API_KEY"
DIFY_EXERCISE_TEMPLATE_API_KEY_ENV = "DIFY_EXERCISE_TEMPLATE_API_KEY"

MAX_VALIDATION_RETRIES = 1
SANITY_INSTANTIATION_COUNT = 5

_VALID_EXERCISE_TYPES = {"mcq", "numeric", "symbolic_math", "fill_blank", "short_answer"}
_VALID_DIFFICULTIES = {"basic", "intermediate", "advanced"}


class TemplateValidationError(Exception):
    """A generated exercise template failed schema or sanity-
    instantiation validation. Message is fed back to Dify/Claude on
    retry (PRD.md: "ONE retry, feeding the validation error back")."""


# Prompt lives in code, not in the Dify app's UI — matches how every
# other LLM prompt in this codebase already works (e.g.
# intent/classifier.py's _FALLBACK_PROMPT): version-controlled,
# diffable, and directly editable/debuggable here rather than requiring
# a manual copy-paste round-trip through a UI with no visibility into
# it. The Dify app itself becomes a thin pass-through: one input
# variable ("prompt"), one LLM node whose prompt is just that variable,
# one output variable. Only the model/provider choice (Gemini) still
# lives in Dify — "swappable independently" (ARCHITECTURE.md) refers to
# that, not to prompt text ownership.
_ACQUIRE_PROMPT = """You are a research assistant building an educational content library.
Find 2-4 high-quality educational resources about: {topic}

Prefer reputable sources (OpenStax, MIT OpenCourseWare, Wikipedia, or similar).
For each, extract or summarize the actual educational content, not just a link.

Respond with ONLY a valid JSON array — no other text, no markdown code fences.
Each object must have exactly these fields:
- title (string)
- url (string, or null)
- author_org (string, or null)
- content (string — the actual educational text/summary)
- license_status (string, or null)
"""


async def acquire(topic: str) -> list[Resource]:
    """AcquisitionProvider.acquire() (ARCHITECTURE.md) — Dify -> Gemini
    (web search + retrieval grounding), used by Flow 5's acquisition
    fallback (PRD.md: OpenStax, MIT OCW, Wikipedia).

    Returns RAW acquired content, not yet chunked/embedded — Rust
    reuses Flow 1's existing ingestion pipeline (chunk + POST /embed)
    for that, same as any other web_fetch source, rather than this
    duplicating it.

    Dify workflow contract assumed (UNVERIFIED, no real Dify app exists
    yet as of this build): ONE input variable, "prompt" (plain text) —
    the fully-assembled prompt built here, not just the bare topic.
    Output variable "resources" — plain text containing a JSON array of
    objects (title, url, author_org, content, license_status). Every
    Dify variable in this project is plain text on both sides of the
    boundary, never array/object-typed — JSON encoding/decoding happens
    entirely in this Python code instead.
    """
    prompt = _ACQUIRE_PROMPT.format(topic=topic)
    outputs = await call_dify_workflow(DIFY_ACQUIRE_API_KEY_ENV, {"prompt": prompt})
    raw_resources = parse_json_output(outputs.get("resources", "[]"))
    return [Resource(**r) for r in raw_resources]


# Same prompt-in-code pattern as acquire()'s _ACQUIRE_PROMPT — see that
# constant's comment for why. Split into a task template + a separate
# output-format block (not passed through .format() itself) purely to
# avoid escaping every literal { and } in the JSON example as {{ }}.
_GENERATE_DAG_TASK_TEMPLATE = """Design a learning path (an ordered sequence of concepts) for a student who wants to learn: {topic}
{intake_block}
For each concept, provide: title, description, difficulty_level (integer 1-5), learning_objective, and prerequisites (a list of OTHER concept titles from this same list that must come before it).
{diagnostic_block}"""

_GENERATE_DAG_OUTPUT_FORMAT = """
Respond with ONLY a single valid JSON object, no other text, no markdown fences:
{
  "concepts": [ ...concept objects as described above... ],
  "entry_concept": "title of the first concept the student should start with",
  "diagnostic_primary": { ...exercise object... } or null,
  "diagnostic_backup": { ...exercise object... } or null
}"""

_DIAGNOSTIC_INSTRUCTIONS = """
Also generate two diagnostic exercises to check whether the student's self-reported level is accurate: a "primary" exercise at their claimed level, and a "backup" one level down. Each exercise object needs: exercise_type (mcq | numeric | symbolic_math | fill_blank | short_answer), difficulty (basic | intermediate | advanced), template_body (object with question_template and answer_template strings -- use {name} placeholders for anything randomized, plus choices_template for mcq), template_params (object mapping each {name} to {"min": x, "max": y}, or null if nothing is randomized), correct_answer, grader_type, grader_config, tolerance."""


def _build_generate_dag_prompt(topic: str, intake_context: IntakeContext | None) -> str:
    if intake_context is not None:
        intake_block = (
            f"\nStudent's self-reported level: {intake_context.level}\n"
            f"Student's goal: {intake_context.goal}\n"
            f"Student's background: {intake_context.background or '(not provided)'}\n"
        )
        diagnostic_block = _DIAGNOSTIC_INSTRUCTIONS
    else:
        intake_block = "\n(No level/goal/background was provided — design a general-purpose path for a beginner.)\n"
        diagnostic_block = "\nSet diagnostic_primary and diagnostic_backup to null — no diagnostic exercises are needed this time."

    task = _GENERATE_DAG_TASK_TEMPLATE.format(topic=topic, intake_block=intake_block, diagnostic_block=diagnostic_block)
    return task + _GENERATE_DAG_OUTPUT_FORMAT


async def generate_dag(topic: str, intake_context: IntakeContext | None = None) -> DAGResult:
    """AcquisitionProvider.generate_dag() (ARCHITECTURE.md) — Dify ->
    Claude (pedagogical concept ordering and prerequisite reasoning).

    When intake_context is provided (Onboarding Diagnostic Step 2, NEW
    subject branch, PRD.md): ONE combined call returns the draft DAG
    PLUS a primary diagnostic exercise at the claimed level and a
    backup one level down — not two sequential Dify calls.

    Prompt built entirely in code (_build_generate_dag_prompt), same
    pattern as acquire() — see _ACQUIRE_PROMPT's comment for why.

    Dify workflow contract assumed (UNVERIFIED): ONE input variable,
    "prompt" (plain text). ONE output variable, "result" — plain text
    containing a single JSON object with keys "concepts" (array
    matching ConceptNode), "entry_concept" (string),
    "diagnostic_primary"/"diagnostic_backup" (objects matching
    ExerciseTemplate, or null when no intake was given). Deliberately
    ONE combined output, not four separate Dify output variables — a
    single LLM call only produces one text response; splitting it into
    multiple named Dify outputs would need an extra Code node. Parsing
    the one blob into its parts happens here instead, keeping the
    actual Dify workflow to just Start -> LLM -> End.
    """
    prompt = _build_generate_dag_prompt(topic, intake_context)
    outputs = await call_dify_workflow(DIFY_DAG_API_KEY_ENV, {"prompt": prompt})
    result = parse_json_output(outputs.get("result", "{}"))

    concepts = [ConceptNode(**c) for c in result.get("concepts", [])]

    diagnostic_primary = None
    diagnostic_backup = None
    if intake_context is not None:
        if result.get("diagnostic_primary"):
            diagnostic_primary = ExerciseTemplate(**result["diagnostic_primary"])
        if result.get("diagnostic_backup"):
            diagnostic_backup = ExerciseTemplate(**result["diagnostic_backup"])

    return DAGResult(
        concepts=concepts,
        entry_concept=result.get("entry_concept", ""),
        diagnostic_primary=diagnostic_primary,
        diagnostic_backup=diagnostic_backup,
    )


def _validate_schema(template: dict) -> None:
    """Validation layer 1/3 (PRD.md, Exercise Template Generation):
    structure, enums, JSON shape valid against the exercises table's
    expected format."""
    exercise_type = template.get("exercise_type")
    if exercise_type not in _VALID_EXERCISE_TYPES:
        raise TemplateValidationError(
            f"exercise_type must be one of {sorted(_VALID_EXERCISE_TYPES)}, got {exercise_type!r}"
        )

    difficulty = template.get("difficulty")
    if difficulty not in _VALID_DIFFICULTIES:
        raise TemplateValidationError(
            f"difficulty must be one of {sorted(_VALID_DIFFICULTIES)}, got {difficulty!r}"
        )

    template_body = template.get("template_body")
    if not isinstance(template_body, dict) or "question_template" not in template_body:
        raise TemplateValidationError("template_body must be an object with at least question_template")


def _instantiate_params(template_params: dict) -> dict:
    """Draws ONE random value per parameter from its {min, max} range.

    Placeholder/range syntax ({name} in templates, {"min": x, "max": y}
    in template_params) is a real design decision made during this
    build — nothing in PRD.md/SCHEMA.md specifies a concrete syntax,
    only that template_params is "generation config e.g. value ranges
    to randomize from". Flagged explicitly in the build report, not
    silently invented; the Dify prompt (setup instructions) must target
    this exact syntax for validation to actually work.
    """
    values = {}
    for name, spec in (template_params or {}).items():
        lo, hi = spec["min"], spec["max"]
        values[name] = random.randint(lo, hi) if isinstance(lo, int) and isinstance(hi, int) else random.uniform(lo, hi)
    return values


def _try_evaluate_answer(filled_answer: str) -> None:
    """For formula-shaped answer templates (numeric/symbolic_math),
    confirm the filled-in expression is actually computable — no
    division by zero, no undefined/infinite result. Safe to sympify()
    directly: this is TRUSTED content (Claude-authored via Dify), not
    student input — Block 9's asteval-based safety layer is specific
    to grading untrusted answers and doesn't apply here.
    """
    try:
        value = sympy.sympify(filled_answer)
        if value.has(sympy.zoo, sympy.nan) or value.is_infinite:
            raise TemplateValidationError(f"answer evaluates to an undefined/infinite value: {filled_answer!r}")
    except (sympy.SympifyError, TypeError, ValueError, ZeroDivisionError) as e:
        raise TemplateValidationError(f"answer template did not evaluate cleanly: {filled_answer!r} ({e})") from e


def _sanity_check_instantiation(template: dict) -> None:
    """Validation layer 2/3 (PRD.md): generate ~5 instances, confirm
    answers are actually computable, no degenerate parameters. Layer
    3/3 (difficulty mislabeling) is explicitly "NOT automatically
    catchable" per PRD.md — out of scope here by design, handled by
    the existing "flag as wrong" mechanism (Rule 32) downstream instead.
    """
    exercise_type = template["exercise_type"]
    question_template = template["template_body"].get("question_template", "")
    answer_template = template["template_body"].get("answer_template", "")
    template_params = template.get("template_params") or {}

    for _ in range(SANITY_INSTANTIATION_COUNT):
        values = _instantiate_params(template_params)
        try:
            question_template.format(**values)
        except (KeyError, ValueError) as e:
            raise TemplateValidationError(f"question_template failed to fill with params {values}: {e}") from e

        if not answer_template:
            continue
        try:
            filled_answer = answer_template.format(**values)
        except (KeyError, ValueError) as e:
            raise TemplateValidationError(f"answer_template failed to fill with params {values}: {e}") from e

        if exercise_type in ("numeric", "symbolic_math"):
            _try_evaluate_answer(filled_answer)


async def generate_exercise_template(
    concept_id: str,
    concept_meta: ConceptMeta,
    top_chunks: list[Chunk],
    batch_children: list[str] | None = None,
) -> list[ExerciseTemplate]:
    """AcquisitionProvider.generate_exercise_template() (ARCHITECTURE.md,
    PRD.md's Exercise Template Generation) — Dify -> Claude (qwen NEVER
    authors templates). Batches the entry concept + up to 3 immediate
    children in ONE Dify call (PRD.md: "same total call count as pure
    lazy... the call just starts earlier").

    Race prevention (the partial UNIQUE index on
    exercises(concept_id, difficulty) WHERE is_canonical, and the Redis
    SET NX lock generating_template:{concept_id}) is RUST's
    responsibility, not this function's — ai_service has no DB or Redis
    access (same boundary as everywhere else in this project). Rust
    decides whether to call this endpoint at all and holds the lock
    around that decision; this function only ever generates+validates.

    Validation — schema + sanity instantiation only (PRD.md's first two
    of three layers). On failure: ONE retry, feeding the error back to
    Dify. If that also fails: fail OPEN — returns an empty list rather
    than raising, so the caller can mark the concept template-pending
    and keep teaching (PRD.md: "Never block the teaching loop on a
    failed template generation").

    Dify workflow contract assumed (UNVERIFIED): input variables, ALL
    plain text — "concept_title", "concept_description", "chunks"
    (JSON-encoded array of {text, chunk_type, difficulty}),
    "batch_children" (JSON-encoded array of concept_ids),
    "validation_error" (empty string on the first attempt). Output
    variable "templates" — plain text containing a JSON array matching
    ExerciseTemplate.
    """
    batch_children = batch_children or []
    inputs: dict = {
        "concept_title": concept_meta.title,
        "concept_description": concept_meta.description,
        "chunks": json.dumps([c.model_dump() for c in top_chunks]),
        "batch_children": json.dumps(batch_children),
    }

    validation_error = ""
    for attempt in range(MAX_VALIDATION_RETRIES + 1):
        inputs["validation_error"] = validation_error
        outputs = await call_dify_workflow(DIFY_EXERCISE_TEMPLATE_API_KEY_ENV, inputs)
        raw_templates = parse_json_output(outputs.get("templates", "[]"))

        try:
            for raw in raw_templates:
                _validate_schema(raw)
                _sanity_check_instantiation(raw)
            return [ExerciseTemplate(**raw) for raw in raw_templates]
        except TemplateValidationError as e:
            validation_error = str(e)
            logger.info(
                "generate_exercise_template: validation failed for concept_id=%s (attempt %d): %s",
                concept_id,
                attempt + 1,
                validation_error,
            )

    logger.info(
        "generate_exercise_template: fail-open for concept_id=%s after %d attempt(s), marking template-pending",
        concept_id,
        MAX_VALIDATION_RETRIES + 1,
    )
    return []
