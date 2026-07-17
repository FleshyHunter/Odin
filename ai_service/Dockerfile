# TEMPLATE ONLY — ai_service/ has no application code yet (that's
# Block 4 of the backend sequencing plan: FastAPI skeleton, GET
# /health, POST /embed). This anticipates the locked Repository
# Structure's shape (requirements.txt + app/main.py exposing a FastAPI
# `app` instance) but CANNOT actually be built until those exist —
# `docker build` will fail on the COPY steps below until then.
FROM python:3.12-slim
WORKDIR /app

COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

COPY app ./app

EXPOSE 8000
CMD ["uvicorn", "app.main:app", "--host", "0.0.0.0", "--port", "8000"]
