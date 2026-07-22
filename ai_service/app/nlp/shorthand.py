import re

from app.nlp.text_utils import tokenize

# Small, closed, well-known set — genuinely solved by a lookup table,
# not "learning" (ARCHITECTURE.md, Input Normalization Step 1). Slang
# coverage here is a partial, honest ceiling — new slang appears
# constantly, a static list is never complete (see the Fine-Tuning
# Roadmap note in the same doc section for the real long-term path).
SHORTHAND_MAP: dict[str, str] = {
    "u": "you",
    "ur": "your",  # doc lists "your/you're" — one deterministic pick
    "idk": "I don't know",
    "lowkey": "somewhat",
    "ngl": "not gonna lie",
}

# "b/c" contains "/", not a word character under tokenize()'s
# definition, so it would never appear as a single token — handled as
# its own substring pattern rather than forcing the shared tokenizer to
# special-case punctuation for one entry.
_SPECIAL_PATTERNS: list[tuple[re.Pattern, str]] = [
    (re.compile(r"\bb/c\b", re.IGNORECASE), "because"),
]


def expand_shorthand(text: str) -> str:
    """Step 1 — exact-match substitution, whole words only (so
    replacing "u" doesn't touch "you", and vice versa)."""
    for pattern, replacement in _SPECIAL_PATTERNS:
        text = pattern.sub(replacement, text)

    tokens = tokenize(text)
    if not tokens:
        return text

    pieces: list[str] = []
    last_end = 0
    for token in tokens:
        replacement = SHORTHAND_MAP.get(token.text.lower())
        pieces.append(text[last_end : token.start])
        pieces.append(replacement if replacement is not None else token.text)
        last_end = token.end
    pieces.append(text[last_end:])
    return "".join(pieces)
