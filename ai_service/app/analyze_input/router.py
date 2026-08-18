from fastapi import APIRouter
from pydantic import BaseModel

from app.analyze_input.service import analyze_input

router = APIRouter()


class AnalyzeInputRequest(BaseModel):
    text: str
    # Subject-scoped: only the current journey's subject's concepts,
    # not the full cross-subject vocabulary bank.
    known_terms: list[str] = []
    # Nullable — the concept the student is currently on, if
    # mid-journey. Presence-only signal (not validated as a real UUID;
    # ai_service has no DB access to check it against anything).
    current_concept_id: str | None = None
    # deferred.md #75/2b — only meaningful alongside current_concept_id;
    # together they locate that concept's own stored embedding for the
    # richer on-topic check (see service.py's own doc comment). None for
    # memoryless mode, same as current_concept_id always was.
    subject_id: str | None = None
    dag_version: int | None = None
    # deferred.md #2b — display names for Stage 2's classify_gap()
    # prompt if it fires. Independent of subject_id/dag_version above:
    # both None is the common case (memoryless mode, or a mid-journey
    # turn that never reaches the ambiguous band).
    subject_title: str | None = None
    current_concept_title: str | None = None


class AnalyzeInputResponse(BaseModel):
    raw_input: str
    cleaned_query: str
    lemmas: list[str]
    keywords: list[str]
    is_on_topic: bool
    matched_concepts: list[str]
    detected_intent: str
    # deferred.md #2b — "on_topic_elsewhere" | "off_topic" | "ambiguous"
    # | "dag_gap" | None. None unless is_on_topic is False AND
    # current_concept_id was present (mid-journey); "ambiguous" means
    # Stage 2 either wasn't reachable (no titles supplied) or failed —
    # not yet consumed by anything (2c/2d), additive only for now.
    gap_classification: str | None = None


@router.post("/analyze_input", response_model=AnalyzeInputResponse)
async def analyze_input_endpoint(request: AnalyzeInputRequest) -> AnalyzeInputResponse:
    result = await analyze_input(
        request.text,
        request.known_terms,
        request.current_concept_id,
        request.subject_id,
        request.dag_version,
        request.subject_title,
        request.current_concept_title,
    )
    return AnalyzeInputResponse(**result)
