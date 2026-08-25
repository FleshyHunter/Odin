import os

import httpx

from app.search.errors import SearchToolError, SearchToolNotConfigured

WOLFRAM_APP_ID_ENV = "WOLFRAM_APP_ID"

# Wolfram Alpha's own "LLM API" (api.wolframalpha.com/v1/llm-api) —
# deliberately NOT the Full Results API (structured pods/XML, meant for
# a UI to render) or the Short Answers API (one bare line, no
# reasoning shown) — the LLM API exists specifically for this use case
# (an LLM agent consuming the result as context for its own answer)
# and returns markdown-formatted, multi-step reasoning already suited
# to that. UNVERIFIED against a real query as of this build — the real
# AppID was registered by a separate Windows-side session (deferred.md
# #93), not confirmed callable from here (no direct network path to
# test with from this machine).
WOLFRAM_LLM_API_URL = "https://www.wolframalpha.com/api/v1/llm-api"


def query_wolfram(query: str) -> str:
    """Computational/factual query via Wolfram Alpha's LLM API — math,
    science, unit conversions, structured facts. Quota-capped (~2,000
    calls/month on the free tier, per the relayed status update this
    was registered under) — not tracked or enforced here, just worth
    knowing if this starts erroring under real traffic.
    """
    app_id = os.environ.get(WOLFRAM_APP_ID_ENV)
    if not app_id:
        raise SearchToolNotConfigured(f"{WOLFRAM_APP_ID_ENV} is not set — Wolfram Alpha is not configured")

    try:
        response = httpx.get(
            WOLFRAM_LLM_API_URL,
            params={"appid": app_id, "input": query},
            timeout=15.0,
            # httpx does NOT follow redirects by default (unlike
            # requests) — a real bug found live in arxiv_client.py's
            # own verification, applied defensively here too.
            follow_redirects=True,
        )
    except httpx.RequestError as e:
        raise SearchToolError(f"Wolfram Alpha request failed: {e}") from e

    # Wolfram's LLM API returns 501 with a plain-text explanation when
    # it genuinely has no answer (not malformed input, not a real
    # error) — a real, expected outcome for some queries, not
    # something to raise on. Every other non-200 is a genuine failure.
    if response.status_code == 501:
        return f"Wolfram Alpha could not answer: {response.text.strip()[:300]}"
    if response.status_code != 200:
        raise SearchToolError(f"Wolfram Alpha returned status {response.status_code}: {response.text[:200]}")

    return response.text.strip()
