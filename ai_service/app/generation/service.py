import os
from functools import lru_cache

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
    )
    return response.message.content
