import os
import tempfile
import threading
from functools import lru_cache
from typing import Any

import whisper
from whisper.audio import SAMPLE_RATE

# Locked model (PRD.md, Voice Input) — base, 74M params, ~1GB VRAM.
# Do NOT swap to a larger Whisper variant without revisiting the VRAM
# budget shared with qwen3.5:9b (see PRD.md's Voice Input section).
MODEL_NAME = "base"

# router.py now offloads transcribe_audio() to a threadpool (needed to
# stop it blocking the event loop — see router.py's own comment), which
# makes genuinely concurrent calls into this function possible for the
# first time. The single lru_cache'd model instance and its underlying
# CUDA context were never built for concurrent inference from multiple
# threads — serializing every real transcribe() call through this lock
# trades "two overlapping chunk-uploads can't transcribe in true
# parallel" for "no CUDA contention/corruption risk" on an already
# GPU-constrained box that's also running Ollama.
_TRANSCRIBE_LOCK = threading.Lock()


@lru_cache(maxsize=1)
def get_model() -> Any:
    return whisper.load_model(MODEL_NAME)


def transcribe_audio(audio_bytes: bytes, filename: str, window_seconds: float | None = None) -> str:
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

        # deferred.md #98 — chunked live-caption calls pass window_seconds
        # so cost stays roughly flat regardless of total recording length
        # (re-transcribing the WHOLE growing buffer every ~4s is what
        # actually caused the reported freeze — cost grows with total
        # length, not just the new audio). None (the one-shot /transcribe
        # call, and the final authoritative call on stop) keeps the
        # original full-file behavior untouched — that one call needs
        # the complete, accurate transcript, not a windowed guess.
        # whisper.load_audio() shells out to ffmpeg the same way
        # transcribe() does internally, just returns the decoded samples
        # instead of feeding them straight to the model — same decode
        # path, no new dependency.
        audio: Any = tmp.name
        if window_seconds is not None:
            samples = whisper.load_audio(tmp.name)
            window_samples = int(window_seconds * SAMPLE_RATE)
            audio = samples[-window_samples:]

        # deferred.md #67 — pinned English, same failure class as the
        # already-fixed langdetect bug (problems.md #25): Whisper's own
        # language-ID is unreliable on short utterances, exactly what a
        # voice question looks like, and PRD.md's Voice Input section
        # locks English-only. Without this, Whisper ran its own
        # auto-detection (and paid for that extra pass) on every clip.
        with _TRANSCRIBE_LOCK:
            result = model.transcribe(audio, language="en")
    return result["text"].strip()
