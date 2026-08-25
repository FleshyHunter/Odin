import os

import httpx

from app.search.errors import SearchToolError

SEARXNG_URL_ENV = "SEARXNG_URL"
# deferred.md #93 — the Windows-side SearXNG deployment is locked to
# 127.0.0.1 only (not reachable via Tailscale/LAN) — deliberately NOT
# used as this default. ai_service itself runs inside its OWN Docker
# container (docker-compose.windows.yml), where `127.0.0.1`/`localhost`
# means the ai_service container itself, not the Windows host or
# SearXNG's own separate container — confirmed against this exact
# compose file's own OLLAMA_HOST/REDIS_URL entries, which already
# document this same gotcha for every other natively/separately-run
# service on this host and reach them via `host.docker.internal`
# instead. Caught here before ever being deployed, not live. Port
# matches the single-container deploy's own `-p 8888:8080` mapping;
# overridable since the real deployment wasn't done from this session
# and the port wasn't independently confirmed here.
DEFAULT_SEARXNG_URL = "http://host.docker.internal:8888"

# A search this broad only needs the top handful of results to be
# useful to the model, and every extra result is extra tokens in the
# follow-up call.
MAX_RESULTS = 5


def search_web(query: str) -> str:
    """General web search via the local SearXNG instance.

    UNVERIFIED against the real deployment as of this build — the
    actual SearXNG instance was stood up by a separate Windows-side
    session, not this one, and this machine has no network path to
    127.0.0.1 on that host to test against directly (see deferred.md
    #93's own note on this). Built against SearXNG's documented JSON
    API shape (?format=json, needs `json` enabled in settings.yml,
    which that session's own status update confirmed was done).
    """
    base_url = os.environ.get(SEARXNG_URL_ENV, DEFAULT_SEARXNG_URL)

    try:
        response = httpx.get(
            f"{base_url}/search",
            params={"q": query, "format": "json"},
            timeout=15.0,
            # httpx does NOT follow redirects by default (unlike
            # requests) — a real bug found live in arxiv_client.py's
            # own verification, applied defensively here too.
            follow_redirects=True,
        )
    except httpx.RequestError as e:
        raise SearchToolError(f"SearXNG request failed: {e}") from e

    if response.status_code != 200:
        raise SearchToolError(f"SearXNG returned status {response.status_code}: {response.text[:200]}")

    body = response.json()
    results = body.get("results", [])[:MAX_RESULTS]
    if not results:
        return f"No web search results found for: {query}"

    lines = [f"Web search results for: {query}"]
    for r in results:
        title = r.get("title", "").strip()
        content = r.get("content", "").strip()
        url = r.get("url", "").strip()
        lines.append(f"- {title}: {content} ({url})")
    return "\n".join(lines)
