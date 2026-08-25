import os
from functools import lru_cache
from typing import Iterator

from ollama import Client

from app.observability import track_generation
from app.search.tools import SEARCH_TOOLS, run_tool

# Currently-adopted operational model (ARCHITECTURE.md, Fine-Tuning
# Roadmap — Qwen3.5-9B adopted ahead of the original evaluation plan,
# see v4.26). A deliberate architecture decision, not a config knob.
MODEL_NAME = "qwen3.5:9b"

# Read at import time (container boot), not lazily inside get_client() —
# a missing env var then fails fast with a clear error at startup,
# instead of a raw KeyError deep in a request handler's stack trace on
# whatever the first /generate call happens to be. Same "fail clearly,
# fail early" philosophy as JWT_SECRET's startup-time env read on the
# Rust side.
OLLAMA_HOST = os.environ["OLLAMA_HOST"]

# Ollama's own default (4096, confirmed via server logs) is too small
# for this model's reasoning phase — live testing reproduced the exact
# pre-/api/chat-fix failure (empty response, reasoning truncated
# mid-thought) purely from running out of context room, not from the
# turn-boundary problem that fix already solved. Configurable, not
# hardcoded, so it can be tuned later without a code change — same
# pattern as OLLAMA_HOST, but with a sensible default since (unlike the
# host) there's a reasonable value to fall back to.
OLLAMA_NUM_CTX = int(os.environ.get("OLLAMA_NUM_CTX", "8192"))


@lru_cache(maxsize=1)
def get_client() -> Client:
    # Client(host=...) only stores connection info, same as
    # redis.Client.open() elsewhere in this project — it never touches
    # the network until a real call is made, so this stays safe to
    # construct from a health check.
    return Client(host=OLLAMA_HOST)


def _build_messages(
    prompt: str, system: str | None, history: list[dict[str, str]] | None = None
) -> list[dict[str, str]]:
    messages: list[dict[str, str]] = []
    if system:
        messages.append({"role": "system", "content": system})
    if history:
        messages.extend(history)
    messages.append({"role": "user", "content": prompt})
    return messages


def generate_text(prompt: str, think: bool = True, system: str | None = None) -> str:
    # /api/chat, not /api/generate — the model's Modelfile TEMPLATE is a
    # bare `{{ .Prompt }}` with no chat-role wrapping, so /api/generate
    # gives it no turn-boundary/stop signal at all. /api/chat applies
    # the model's real chat template (confirmed present in its own
    # server logs), so it knows where a turn is supposed to end instead
    # of rambling until it's forcibly truncated at the context limit.
    #
    # think defaults True: qwen3.5:9b is a reasoning model, and the
    # whole point of adopting it over qwen2.5:7b was reasoning depth
    # (Fine-Tuning Roadmap) — defaulting it off to save time would
    # quietly undo that. Once Block 8's intent classification exists,
    # callers can pass a per-message decision instead of this default.
    #
    # system is optional and None by default — intent/classifier.py's
    # own direct call to this function (its qwen fallback) deliberately
    # passes none, so this stays a no-op for every existing caller
    # unless one opts in (see generation/router.py, the one caller that
    # does, for the English-only enforcement instruction).
    with track_generation():
        client = get_client()
        response = client.chat(
            model=MODEL_NAME,
            messages=_build_messages(prompt, system),
            think=think,
            options={"num_ctx": OLLAMA_NUM_CTX},
        )
        return response.message.content


# Backend follow-up to Block 11 (markdown/deferred.md #20): qwen3.5:9b's
# "thinking" mode has load-dependent generation time, confirmed live
# during Block 11's own verification (one /generate call took 22s,
# another exceeded 120s). Streaming lets a caller start showing output
# long before the full response is done, instead of one long blocking
# wait. ollama's own client already supports stream=True natively —
# each yielded chunk carries the same Message shape as the blocking
# call, just with `content` holding an incremental delta rather than
# the full text. The empty-content guard skips ollama's own boundary
# chunks (e.g. the final done=True chunk often carries no new text).
def generate_text_stream(
    prompt: str,
    think: bool = True,
    system: str | None = None,
    history: list[dict[str, str]] | None = None,
) -> Iterator[str]:
    with track_generation():
        client = get_client()
        stream = client.chat(
            model=MODEL_NAME,
            messages=_build_messages(prompt, system, history),
            think=think,
            options={"num_ctx": OLLAMA_NUM_CTX},
            stream=True,
        )
        for chunk in stream:
            content = chunk.message.content
            if content:
                yield content


# deferred.md #93 — real-model-verified (this session, against the
# actual Ollama host) before being written: streaming + tools work
# together, and critically, when think=True and the model DOES call a
# tool, no visible text streams first — the tool-call decision resolves
# silently within the same "thinking" delay every reasoning-mode call
# already pays, so offering tools costs nothing extra on the (common)
# no-tool path versus generate_text_stream() above. Deliberately a
# SEPARATE function, not a parameter added to generate_text_stream()
# itself — some callers (e.g. journeys/turn.rs's branch-confirmation
# acknowledgment) want a short, deterministic reply and should never
# have a search tool available to reach for, same "scope new behavior
# narrowly" reasoning generation/router.py's own _SYSTEM_PROMPT already
# follows (folded into specific endpoints, not every qwen caller).
def generate_text_stream_with_tools(
    prompt: str,
    think: bool = True,
    system: str | None = None,
    history: list[dict[str, str]] | None = None,
) -> Iterator[str]:
    with track_generation():
        client = get_client()
        messages = _build_messages(prompt, system, history)
        stream = client.chat(
            model=MODEL_NAME,
            messages=messages,
            think=think,
            tools=SEARCH_TOOLS,
            options={"num_ctx": OLLAMA_NUM_CTX},
            stream=True,
        )

        tool_call = None
        for chunk in stream:
            content = chunk.message.content
            if content:
                yield content
            calls = getattr(chunk.message, "tool_calls", None)
            if calls:
                # v1 scope: only the first tool call in a response is
                # acted on. Not observed to request more than one in
                # real testing; revisit if that changes.
                tool_call = calls[0]
                break

        if tool_call is None:
            # No tool call — the stream above already WAS the real
            # answer, nothing more to do. This is the common path and
            # it's identical in shape/cost to generate_text_stream().
            return

        tool_result = run_tool(tool_call.function.name, dict(tool_call.function.arguments))

        # Ollama's own expected shape for feeding a tool result back —
        # verified directly against the real model before this was
        # written (see the diagnostic this entry references): an
        # assistant message carrying the tool_calls it made, followed
        # by a tool-role message carrying the result.
        messages.append({"role": "assistant", "tool_calls": [tool_call]})
        messages.append({"role": "tool", "content": tool_result})

        follow_up = client.chat(
            model=MODEL_NAME,
            messages=messages,
            think=think,
            options={"num_ctx": OLLAMA_NUM_CTX},
            stream=True,
        )
        for chunk in follow_up:
            content = chunk.message.content
            if content:
                yield content
