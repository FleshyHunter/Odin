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


class AnalyzeInputResponse(BaseModel):
    raw_input: str
    cleaned_query: str
    lemmas: list[str]
    keywords: list[str]
    is_on_topic: bool
    matched_concepts: list[str]
    detected_intent: str


@router.post("/analyze_input", response_model=AnalyzeInputResponse)
def analyze_input_endpoint(request: AnalyzeInputRequest) -> AnalyzeInputResponse:
    result = analyze_input(
        request.text,
        request.known_terms,
        request.current_concept_id,
        request.subject_id,
        request.dag_version,
    )
    return AnalyzeInputResponse(**result)
