from fastapi import APIRouter, HTTPException
from pydantic import BaseModel

from app.acquisition.dify_client import DifyError, DifyNotConfigured
from app.acquisition.models import Chunk, ConceptMeta, DAGResult, ExerciseTemplate, IntakeContext, Resource
from app.acquisition.service import acquire, generate_dag, generate_exercise_template

router = APIRouter()


class AcquireRequest(BaseModel):
    topic: str


class AcquireResponse(BaseModel):
    resources: list[Resource]


class GenerateDagRequest(BaseModel):
    topic: str
    intake_context: IntakeContext | None = None


class GenerateExerciseTemplateRequest(BaseModel):
    concept_id: str
    concept_meta: ConceptMeta
    top_chunks: list[Chunk]
    batch_children: list[str] = []


class GenerateExerciseTemplateResponse(BaseModel):
    templates: list[ExerciseTemplate]


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
