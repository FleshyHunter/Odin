class SearchToolError(Exception):
    """A search tool call failed — missing config, network error, or a
    non-2xx response. Mirrors acquisition/dify_client.py's own
    DifyError/DifyNotConfigured shape (same "config checked at call
    time, not import time" reasoning — none of these three tools are
    required for ai_service to boot, matching Dify's own posture)."""


class SearchToolNotConfigured(SearchToolError):
    """The env var this tool needs isn't set. Checked at call time so a
    missing Wolfram/SearXNG/arXiv config never blocks container boot or
    any OTHER endpoint — only a turn that actually needs this one tool
    fails, and it fails soft (see generation/service.py's own caller)."""
