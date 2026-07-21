from fastapi import APIRouter, HTTPException, UploadFile
from pydantic import BaseModel

from app.voice.service import transcribe_audio

router = APIRouter()


class TranscribeResponse(BaseModel):
    text: str


@router.post("/transcribe", response_model=TranscribeResponse)
async def transcribe(file: UploadFile) -> TranscribeResponse:
    audio_bytes = await file.read()
    try:
        text = transcribe_audio(audio_bytes, file.filename or "audio.webm")
    except Exception as e:
        raise HTTPException(status_code=422, detail=f"could not process audio file: {e}") from e
    return TranscribeResponse(text=text)
