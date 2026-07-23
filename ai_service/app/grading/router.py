from fastapi import APIRouter, HTTPException
from pydantic import BaseModel

from app.grading.service import grade_exercise

router = APIRouter()


class GradeRequest(BaseModel):
    exercise_type: str  # exercises.exercise_type — mcq/numeric/symbolic_math/fill_blank/short_answer
    student_answer: str
    correct_answer: str | None = None  # exercises.correct_answer (or quiz_attempts.expected_answer)
    tolerance: float | None = None  # exercises.tolerance — numeric only
    grader_config: dict | None = None  # exercises.grader_config — fill_blank/short_answer


class GradeResponse(BaseModel):
    is_correct: bool
    score: float
    feedback: str | None = None


@router.post("/grade", response_model=GradeResponse)
def grade(request: GradeRequest) -> GradeResponse:
    try:
        result = grade_exercise(
            request.exercise_type,
            request.student_answer,
            request.correct_answer,
            request.tolerance,
            request.grader_config,
        )
    except ValueError as e:
        raise HTTPException(status_code=422, detail=str(e)) from e
    return GradeResponse(is_correct=result.is_correct, score=result.score, feedback=result.feedback)
