from fastapi import APIRouter, Form, HTTPException, UploadFile
from pydantic import BaseModel

from app.ingestion.service import ingest

router = APIRouter()


class ChunkOut(BaseModel):
    text: str
    token_count: int
    embedding: list[float]


class IngestResponse(BaseModel):
    extracted_text: str
    rejected: bool
    rejection_reason: str | None
    chunks: list[ChunkOut]


@router.post("/ingest", response_model=IngestResponse)
async def ingest_upload(file: UploadFile, role: str = Form(...)) -> IngestResponse:
    if role not in ("ephemeral", "prompt_upload", "material_upload"):
        raise HTTPException(status_code=422, detail=f"invalid role: {role}")

    data = await file.read()
    try:
        result = ingest(data, file.filename or "upload", role)
    except Exception as e:
        raise HTTPException(status_code=422, detail=f"could not process upload: {e}") from e

    return IngestResponse(
        extracted_text=result.extracted_text,
        rejected=result.rejected,
        rejection_reason=result.rejection_reason,
        chunks=[
            ChunkOut(text=c.text, token_count=c.token_count, embedding=c.embedding)
            for c in result.chunks
        ],
    )
