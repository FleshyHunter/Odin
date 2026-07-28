FROM python:3.12-slim
WORKDIR /app

# ffmpeg is a system binary, not a pip package — Whisper (Block 7) shells
# out to it internally to decode uploaded audio before transcribing.
# python:3.12-slim doesn't include it; without this, transcription fails
# deep inside Whisper's decode step, not at boot.
# tesseract-ocr (Block 12): pytesseract only wraps this binary, the OCR
# engine itself is not a pip package either. English-only language data
# is all that's needed (Rule: v1 supports English only).
RUN apt-get update && apt-get install -y --no-install-recommends ffmpeg tesseract-ocr \
    && rm -rf /var/lib/apt/lists/*

COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

# spaCy's model is a separate download, not part of the `spacy` pip
# package itself (Block 8, Step 5 — nlp/spacy_pipe.py).
RUN python -m spacy download en_core_web_sm

COPY app ./app

EXPOSE 8000
CMD ["uvicorn", "app.main:app", "--host", "0.0.0.0", "--port", "8000"]
