import json
import os

import httpx

DIFY_BASE_URL_ENV = "DIFY_BASE_URL"
DEFAULT_DIFY_BASE_URL = "https://api.dify.ai"


class DifyError(Exception):
    """A Dify call failed — missing config, network error, non-200
    status, or a workflow that completed with status != 'succeeded'."""


class DifyNotConfigured(DifyError):
    """The specific API key env var this call needs isn't set. Checked
    at CALL time, not import time — unlike OLLAMA_HOST (read eagerly
    at module import in generation/service.py), Dify isn't set up yet
    as of this build, and the rest of ai_service (/generate,
    /analyze_input, /grade, etc.) must keep working regardless. An
    eager read here would crash container boot for every endpoint,
    not just these three.
    """


async def call_dify_workflow(api_key_env: str, inputs: dict) -> dict:
    """Calls a Dify Workflow app's POST /v1/workflows/run (blocking
    mode) and returns its `outputs` dict.

    UNVERIFIED against a real Dify instance as of this build — no Dify
    account/API key exists yet. Built against Dify's own documented
    Workflow API: POST {base_url}/v1/workflows/run, Authorization:
    Bearer {api_key}, body {inputs, response_mode: "blocking", user}.
    Chose the Workflow API over Dify's Chat Messages API specifically
    because Workflow apps return STRUCTURED `outputs` (defined
    variables) rather than one freeform chat `answer` string —
    matches what acquire()/generate_dag()/generate_exercise_template()
    all actually need. This must be the first thing re-verified once a
    real Dify app exists (see setup instructions).

    api_key_env: the env var NAME to read (e.g. "DIFY_ACQUIRE_API_KEY"),
    not the key itself — each of the three callers uses a different
    Dify app/API key (acquire->Gemini, generate_dag/generate_exercise_
    template->Claude — different Dify apps, ARCHITECTURE.md's
    AcquisitionProvider Interface), so this stays a shared, thin
    caller rather than duplicated per-method HTTP logic.
    """
    api_key = os.environ.get(api_key_env)
    if not api_key:
        raise DifyNotConfigured(f"{api_key_env} is not set — Dify is not configured yet")

    base_url = os.environ.get(DIFY_BASE_URL_ENV, DEFAULT_DIFY_BASE_URL)
    url = f"{base_url}/v1/workflows/run"

    try:
        async with httpx.AsyncClient(timeout=120.0) as client:
            response = await client.post(
                url,
                headers={"Authorization": f"Bearer {api_key}"},
                json={"inputs": inputs, "response_mode": "blocking", "user": "odin-ai-service"},
            )
    except httpx.RequestError as e:
        raise DifyError(f"Dify request failed: {e}") from e

    if response.status_code != 200:
        raise DifyError(f"Dify returned status {response.status_code}: {response.text}")

    body = response.json()
    data = body.get("data", {})
    if data.get("status") != "succeeded":
        raise DifyError(f"Dify workflow did not succeed: {data.get('error') or body}")

    return data.get("outputs", {})


def parse_json_output(value: object) -> list | dict:
    """Normalizes one Dify output variable into parsed JSON.

    UNVERIFIED which shape Dify's End node actually hands back for a
    given workflow build — it depends on whether the value is wired
    straight from an LLM node's raw text (a JSON STRING the model
    printed) or through an intermediate Code node that already parsed
    it (a native list/dict). Rather than force the setup instructions
    to require a Code node in every one of the 3 workflows just to
    guarantee one specific shape, this accepts either — the simplest
    possible Dify workflow (Start -> LLM -> End, nothing else) already
    works with this.
    """
    if isinstance(value, (list, dict)):
        return value
    if isinstance(value, str):
        return json.loads(value)
    raise DifyError(f"expected a JSON string, list, or dict output, got {type(value).__name__}: {value!r}")
