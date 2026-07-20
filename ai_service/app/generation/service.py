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


def generate_text(prompt: str) -> str:
    client = get_client()
    response = client.generate(model=MODEL_NAME, prompt=prompt)
    return response.response
