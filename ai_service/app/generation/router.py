from fastapi import APIRouter
from pydantic import BaseModel

from app.generation.service import generate_text

router = APIRouter()


class GenerateRequest(BaseModel):
    prompt: str
    think: bool = True


class GenerateResponse(BaseModel):
    response: str


@router.post("/generate", response_model=GenerateResponse)
def generate(request: GenerateRequest) -> GenerateResponse:
    return GenerateResponse(response=generate_text(request.prompt, request.think))
