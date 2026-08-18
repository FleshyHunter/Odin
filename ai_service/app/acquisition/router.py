from fastapi import APIRouter, HTTPException
from pydantic import BaseModel

from app.acquisition.dify_client import DifyError, DifyNotConfigured
from app.acquisition.models import (
    Chunk,
    ConceptMeta,
    DAGResult,
    ExerciseTemplate,
    FoldedConcept,
    GapClassificationResult,
    IntakeContext,
    Resource,
)
from app.acquisition.service import (
    TemplateValidationError,
    acquire,
    adjust_dag,
    classify_gap,
    fold_concept_into_dag,
    generate_dag,
    generate_exercise_template,
)

router = APIRouter()


class AcquireRequest(BaseModel):
    topic: str


class AcquireResponse(BaseModel):
    resources: list[Resource]


class GenerateDagRequest(BaseModel):
    topic: str
    intake_context: IntakeContext | None = None


class AdjustDagRequest(BaseModel):
    topic: str
    draft: DAGResult
    reason: str


class GenerateExerciseTemplateRequest(BaseModel):
    concept_id: str
    concept_meta: ConceptMeta
    top_chunks: list[Chunk]
    batch_children: list[str] = []


class GenerateExerciseTemplateResponse(BaseModel):
    templates: list[ExerciseTemplate]


class ClassifyGapRequest(BaseModel):
    subject_title: str
    concept_titles: list[str]
    current_concept_title: str
    reply_text: str


class FoldConceptRequest(BaseModel):
    subject_title: str
    concept_titles: list[str]
    current_concept_title: str
    reply_text: str


def _dify_error_to_http(e: Exception) -> HTTPException:
    # 503: Dify isn't configured yet (env var missing) — a config gap,
    # not an upstream failure. 502: Dify itself was reachable but
    # failed/errored — an upstream problem, distinct from "not set up".
    if isinstance(e, DifyNotConfigured):
        return HTTPException(status_code=503, detail=str(e))
    return HTTPException(status_code=502, detail=str(e))


@router.post("/acquire", response_model=AcquireResponse)
async def acquire_endpoint(request: AcquireRequest) -> AcquireResponse:
    try:
        resources = await acquire(request.topic)
    except DifyError as e:
        raise _dify_error_to_http(e) from e
    return AcquireResponse(resources=resources)


@router.post("/generate_dag", response_model=DAGResult)
async def generate_dag_endpoint(request: GenerateDagRequest) -> DAGResult:
    try:
        return await generate_dag(request.topic, request.intake_context)
    except DifyError as e:
        raise _dify_error_to_http(e) from e
    except TemplateValidationError as e:
        # deferred.md #6 — a dangling prerequisite reference survived
        # the retry-once loop. Unlike a bad diagnostic exercise, there's
        # no DAG left to fail open to — a real 422 the caller must
        # handle, not a 200 with something silently broken inside it.
        raise HTTPException(status_code=422, detail=str(e)) from e


@router.post("/adjust_dag", response_model=DAGResult)
async def adjust_dag_endpoint(request: AdjustDagRequest) -> DAGResult:
    try:
        return await adjust_dag(request.topic, request.draft, request.reason)
    except DifyError as e:
        raise _dify_error_to_http(e) from e
    except TemplateValidationError as e:
        raise HTTPException(status_code=422, detail=str(e)) from e


@router.post("/generate_exercise_template", response_model=GenerateExerciseTemplateResponse)
async def generate_exercise_template_endpoint(
    request: GenerateExerciseTemplateRequest,
) -> GenerateExerciseTemplateResponse:
    try:
        templates = await generate_exercise_template(
            request.concept_id, request.concept_meta, request.top_chunks, request.batch_children
        )
    except DifyError as e:
        raise _dify_error_to_http(e) from e
    return GenerateExerciseTemplateResponse(templates=templates)


@router.post("/classify_gap", response_model=GapClassificationResult)
async def classify_gap_endpoint(request: ClassifyGapRequest) -> GapClassificationResult:
    # deferred.md #2b Stage 2 — exposed as its own endpoint for
    # consistency with every other AcquisitionProvider method (and
    # independent testability/callers), even though analyze_input's own
    # use is an in-process function call, not HTTP (both live in this
    # same FastAPI app).
    try:
        return await classify_gap(
            request.subject_title, request.concept_titles, request.current_concept_title, request.reply_text
        )
    except DifyError as e:
        raise _dify_error_to_http(e) from e


@router.post("/fold_concept", response_model=FoldedConcept)
async def fold_concept_endpoint(request: FoldConceptRequest) -> FoldedConcept:
    # deferred.md #2c — same "own endpoint for consistency + independent
    # testability" reasoning as /classify_gap above.
    try:
        return await fold_concept_into_dag(
            request.subject_title, request.concept_titles, request.current_concept_title, request.reply_text
        )
    except DifyError as e:
        raise _dify_error_to_http(e) from e
