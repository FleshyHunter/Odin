from app.search.arxiv_client import search_arxiv
from app.search.errors import SearchToolError
from app.search.searxng_client import search_web
from app.search.wolfram_client import query_wolfram

# deferred.md #93 — three tools, not one, deliberately NOT pre-filtered
# by subject before the model sees them: real-model testing (this
# session, against the actual Ollama host) confirmed qwen3.5:9b
# reliably judges when a tool is warranted at all AND which one fits,
# given honest descriptions of what each is actually for — a hard
# subject-based preselect isn't needed and would block real edge cases
# (e.g. a math-subject question about the historical mathematician who
# discovered something, which needs web search, not computation).
SEARCH_TOOLS = [
    {
        "type": "function",
        "function": {
            "name": "search_web",
            "description": (
                "Search the live web for current, real-time, or recent information "
                "that would not be reliably known otherwise — e.g. today's news, a "
                "specific person's recent public statements or posts, current "
                "prices, or anything time-sensitive. Do NOT use this for general "
                "academic knowledge, computation, or anything answerable directly."
            ),
            "parameters": {
                "type": "object",
                "properties": {"query": {"type": "string", "description": "The search query."}},
                "required": ["query"],
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "query_wolfram",
            "description": (
                "Compute or verify a math/science answer, convert units, or look up "
                "a structured factual/scientific value — e.g. solve an equation, "
                "compute an integral, convert 5 miles to km, find a chemical "
                "element's atomic weight. Use this to VERIFY a computation you're "
                "not fully certain of, not for questions with no computational "
                "answer (history, literature, current events)."
            ),
            "parameters": {
                "type": "object",
                "properties": {"query": {"type": "string", "description": "The computation or question."}},
                "required": ["query"],
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "search_arxiv",
            "description": (
                "Search academic/research papers on arXiv — STEM research literature "
                "specifically (physics, math, CS, etc). Use only when the student "
                "asks about actual research papers or wants to go beyond textbook "
                "level into current research on a topic, not for general questions."
            ),
            "parameters": {
                "type": "object",
                "properties": {"query": {"type": "string", "description": "The research topic/query."}},
                "required": ["query"],
            },
        },
    },
]

_DISPATCH = {
    "search_web": search_web,
    "query_wolfram": query_wolfram,
    "search_arxiv": search_arxiv,
}


def run_tool(name: str, arguments: dict) -> str:
    """Runs a tool by name and always returns a string result to feed
    back to the model — never raises. A failed/misconfigured tool
    becomes a plain-language error string instead (same fail-soft
    posture as Dify's own callers): the follow-up generation call still
    happens, and the model can tell the student it couldn't look
    something up rather than the whole turn erroring out.
    """
    handler = _DISPATCH.get(name)
    if handler is None:
        return f"Error: unknown tool '{name}'"

    query = arguments.get("query", "")
    try:
        return handler(query)
    except SearchToolError as e:
        return f"Error: {e}"
