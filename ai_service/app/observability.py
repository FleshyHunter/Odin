# Observability — GPU Queuing (ARCHITECTURE.md, locked): GET /health
# reports active_generations, a live count of in-flight Ollama
# generation calls, as an at-a-glance GPU-contention signal.
#
# A plain unlocked int would have a REAL race here, not just a
# theoretical one: /generate's endpoint is a sync `def`, which FastAPI
# runs in its threadpool (real OS threads), not on the asyncio event
# loop — two concurrent requests can genuinely increment/decrement this
# from different threads at the same instant.

import threading
from contextlib import contextmanager

_lock = threading.Lock()
_active_generations = 0


@contextmanager
def track_generation():
    """Wraps one generation call (blocking or streaming). Safe around a
    generator's yield loop too — a `with` block's __exit__ still runs
    correctly whether the generator finishes normally, raises, or is
    closed early (client disconnect, garbage collection)."""
    global _active_generations
    with _lock:
        _active_generations += 1
    try:
        yield
    finally:
        with _lock:
            _active_generations -= 1


def get_active_generations() -> int:
    with _lock:
        return _active_generations
