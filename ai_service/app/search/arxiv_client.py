from xml.etree import ElementTree

import httpx

from app.search.errors import SearchToolError

# arXiv's real, official, keyless API — export.arxiv.org, not
# arxiv.org itself (the export subdomain is the documented API
# endpoint). Atom XML response, not JSON — arXiv has never offered a
# JSON API. https, not http — arXiv 301-redirects http to https, and
# httpx does NOT follow redirects by default (unlike requests, where
# that's the default) — found live via a real query during this
# session's own verification pass, not assumed.
ARXIV_API_URL = "https://export.arxiv.org/api/query"

ATOM_NS = "{http://www.w3.org/2005/Atom}"
MAX_RESULTS = 5


def search_arxiv(query: str) -> str:
    """Academic paper search via arXiv's official API — STEM research
    literature specifically, not general web search."""
    try:
        response = httpx.get(
            ARXIV_API_URL,
            params={"search_query": f"all:{query}", "start": 0, "max_results": MAX_RESULTS},
            timeout=15.0,
            follow_redirects=True,
        )
    except httpx.RequestError as e:
        raise SearchToolError(f"arXiv request failed: {e}") from e

    if response.status_code != 200:
        raise SearchToolError(f"arXiv returned status {response.status_code}: {response.text[:200]}")

    root = ElementTree.fromstring(response.text)
    entries = root.findall(f"{ATOM_NS}entry")
    if not entries:
        return f"No arXiv papers found for: {query}"

    lines = [f"arXiv papers for: {query}"]
    for entry in entries:
        title = (entry.findtext(f"{ATOM_NS}title") or "").strip().replace("\n", " ")
        summary = (entry.findtext(f"{ATOM_NS}summary") or "").strip().replace("\n", " ")
        link = (entry.findtext(f"{ATOM_NS}id") or "").strip()
        lines.append(f"- {title}: {summary[:200]}... ({link})")
    return "\n".join(lines)
