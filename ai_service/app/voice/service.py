import os
import tempfile
from functools import lru_cache
from typing import Any

import whisper

# Locked model (PRD.md, Voice Input) — base, 74M params, ~1GB VRAM.
# Do NOT swap to a larger Whisper variant without revisiting the VRAM
# budget shared with qwen3.5:9b (see PRD.md's Voice Input section).
MODEL_NAME = "base"


@lru_cache(maxsize=1)
def get_model() -> Any:
    return whisper.load_model(MODEL_NAME)


def transcribe_audio(audio_bytes: bytes, filename: str) -> str:
    # Whisper's transcribe() wants a file path, not raw bytes — it shells
    # out to ffmpeg internally to decode whatever format arrives, so a
    # temp file is the simplest correct bridge, not extra ceremony. The
    # suffix is derived from the real upload, not hardcoded to .webm —
    # ffmpeg partly trusts the extension for format detection, and a
    # mismatched one (found via testing with an AIFF file) can confuse
    # it. Decode failures (corrupt/unsupported audio) propagate as a
    # plain exception here — router.py is what translates that into a
    # clean HTTP error, keeping this module FastAPI-free like the other
    # ai_service modules.
    model = get_model()
    suffix = os.path.splitext(filename)[1] or ".webm"
    with tempfile.NamedTemporaryFile(suffix=suffix) as tmp:
        tmp.write(audio_bytes)
        tmp.flush()
        result = model.transcribe(tmp.name)
    return result["text"].strip()
