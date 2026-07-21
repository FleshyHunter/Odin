FROM python:3.12-slim
WORKDIR /app

# ffmpeg is a system binary, not a pip package — Whisper (Block 7) shells
# out to it internally to decode uploaded audio before transcribing.
# python:3.12-slim doesn't include it; without this, transcription fails
# deep inside Whisper's decode step, not at boot.
RUN apt-get update && apt-get install -y --no-install-recommends ffmpeg \
    && rm -rf /var/lib/apt/lists/*

COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

COPY app ./app

EXPOSE 8000
CMD ["uvicorn", "app.main:app", "--host", "0.0.0.0", "--port", "8000"]
