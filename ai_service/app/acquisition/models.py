from pydantic import BaseModel


class Resource(BaseModel):
    """acquire()'s output — raw acquired web content, NOT yet chunked
    or embedded. Reuses Flow 1's existing chunk+embed ingestion
    pipeline for that (same as any other source), rather than
    duplicating it here. Field names mirror SCHEMA.md's sources table
    (source_type='web_fetch' for anything from this path).
    """

    title: str
    url: str | None = None
    author_org: str | None = None
    content: str
    license_status: str | None = None


class IntakeContext(BaseModel):
    """Onboarding Diagnostic Step 1's structured intake (PRD.md) — the
    only input generate_dag() gets beyond the bare topic string, used
    to scope the DAG against an actual level/goal instead of guessing.
    """

    level: str  # "Beginner" | "Intermediate" | "Advanced"
    goal: str  # "Exam prep" | "Project" | "General understanding"
    background: str | None = None


class ConceptMeta(BaseModel):
    """Grounded input for generate_exercise_template() — title +
    description, never a bare concept_id (ARCHITECTURE.md)."""

    title: str
    description: str


class Chunk(BaseModel):
    """The same retrieved-context shape explanations use (ARCHITECTURE.md) —
    passed into generate_exercise_template() so templates are grounded
    in what was actually taught, not just the concept's title."""

    text: str
    chunk_type: str | None = None  # SCHEMA.md chunks.chunk_type
    difficulty: str | None = None  # SCHEMA.md chunks.difficulty


class ConceptNode(BaseModel):
    """One node of a generated DAG — mirrors subject_concepts +
    canonical_concepts' relevant fields. prerequisites are TITLES
    within this same DAG, not concept_ids — concept_ids don't exist
    yet until Rust persists these rows (canonical_concepts.concept_id
    is DB-generated)."""

    title: str
    description: str
    difficulty_level: int  # subject_concepts.difficulty_level, 1-5
    learning_objective: str | None = None
    prerequisites: list[str] = []


class ExerciseTemplate(BaseModel):
    """generate_exercise_template()'s output — mirrors SCHEMA.md's
    exercises table columns directly (this is what Rust inserts,
    verbatim, as one row per template)."""

    exercise_type: str  # mcq | numeric | symbolic_math | fill_blank | short_answer
    difficulty: str  # basic | intermediate | advanced
    template_body: dict  # question_template, answer_template, choices_template, solution_template
    template_params: dict | None = None
    correct_answer: str | None = None
    grader_type: str | None = None
    grader_config: dict | None = None
    tolerance: float | None = None


class DAGResult(BaseModel):
    """generate_dag()'s output. diagnostic_primary/diagnostic_backup
    are populated ONLY when intake_context was provided (Onboarding
    Diagnostic Step 2's "new subject" branch, PRD.md) — one combined
    Dify call producing the draft DAG AND both diagnostic exercises
    together, not two sequential calls.
    """

    concepts: list[ConceptNode]
    entry_concept: str  # title of the starting concept within `concepts`
    diagnostic_primary: ExerciseTemplate | None = None
    diagnostic_backup: ExerciseTemplate | None = None  # one level down from primary
