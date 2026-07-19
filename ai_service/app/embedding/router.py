from fastapi import APIRouter
from pydantic import BaseModel

from app.embedding.service import embed_texts

router = APIRouter()


class EmbedRequest(BaseModel):
    texts: list[str]


class EmbedResponse(BaseModel):
    embeddings: list[list[float]]


@router.post("/embed", response_model=EmbedResponse)
def embed(request: EmbedRequest) -> EmbedResponse:
    return EmbedResponse(embeddings=embed_texts(request.texts))
