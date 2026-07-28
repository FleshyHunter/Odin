import os
from functools import lru_cache
from typing import Iterator

from ollama import Client

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


def generate_text(prompt: str, think: bool = True) -> str:
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
    client = get_client()
    response = client.chat(
        model=MODEL_NAME,
        messages=[{"role": "user", "content": prompt}],
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
def generate_text_stream(prompt: str, think: bool = True) -> Iterator[str]:
    client = get_client()
    stream = client.chat(
        model=MODEL_NAME,
        messages=[{"role": "user", "content": prompt}],
        think=think,
        options={"num_ctx": OLLAMA_NUM_CTX},
        stream=True,
    )
    for chunk in stream:
        content = chunk.message.content
        if content:
            yield content
