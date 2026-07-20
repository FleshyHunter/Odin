import os
from functools import lru_cache

from ollama import Client

# Currently-adopted operational model (ARCHITECTURE.md, Fine-Tuning
# Roadmap — Qwen3.5-9B adopted ahead of the original evaluation plan,
# see v4.26). A deliberate architecture decision, not a config knob.
MODEL_NAME = "qwen3.5:9b"


@lru_cache(maxsize=1)
def get_client() -> Client:
    # Client(host=...) only stores connection info, same as
    # redis.Client.open() elsewhere in this project — it never touches
    # the network until a real call is made, so this stays safe to
    # construct from a health check.
    host = os.environ["OLLAMA_HOST"]
    return Client(host=host)


def generate_text(prompt: str) -> str:
    client = get_client()
    response = client.generate(model=MODEL_NAME, prompt=prompt)
    return response.response
