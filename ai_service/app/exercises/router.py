from fastapi import APIRouter, HTTPException
from pydantic import BaseModel

from app.exercises.service import ExerciseInstantiationError, instantiate_exercise

router = APIRouter()


class InstantiateExerciseRequest(BaseModel):
    exercise_type: str
    template_body: dict
    template_params: dict | None = None
    correct_answer: str | None = None


class InstantiateExerciseResponse(BaseModel):
    rendered_question: str
    instantiated_params: dict
    expected_answer: str | None
    rendered_choices: list[str] | None


@router.post("/instantiate_exercise", response_model=InstantiateExerciseResponse)
def instantiate_exercise_endpoint(request: InstantiateExerciseRequest) -> InstantiateExerciseResponse:
    # Deliberately plain `def`, not `async def` — this is pure CPU-bound
    # work (random draws + string .format() calls), no I/O, no await
    # anywhere in the call chain. Keeping it sync lets FastAPI/Starlette
    # offload it to the threadpool automatically, same posture as
    # embedding/grading/knowledge's own endpoints — see deferred.md's
    # own audit finding on analyze_input_endpoint for why this matters
    # (an async def with no real awaits blocks the single event loop
    # instead of getting that offload).
    try:
        result = instantiate_exercise(
            request.exercise_type, request.template_body, request.template_params, request.correct_answer
        )
    except ExerciseInstantiationError as e:
        raise HTTPException(status_code=422, detail=str(e)) from e
    return InstantiateExerciseResponse(
        rendered_question=result.rendered_question,
        instantiated_params=result.instantiated_params,
        expected_answer=result.expected_answer,
        rendered_choices=result.rendered_choices,
    )
