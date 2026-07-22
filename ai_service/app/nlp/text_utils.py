import re
from dataclasses import dataclass

_WORD_RE = re.compile(r"[A-Za-z0-9']+")


@dataclass
class Token:
    text: str  # the word as it actually appears (original case)
    start: int  # start index in the original string
    end: int  # end index (exclusive) in the original string


def tokenize(text: str) -> list[Token]:
    """Splits text into word tokens with their original positions.

    Shared by shorthand.py (Step 1, whole-word substitution) and
    fuzzy_match.py (Steps 2 & 4, vocabulary matching) — both need the
    same definition of "what counts as a word" and where it sits in
    the string, so a span one step marks lines up with what the next
    step sees. Factored out here rather than each step writing its own
    slightly-different tokenizer.
    """
    return [Token(m.group(), m.start(), m.end()) for m in _WORD_RE.finditer(text)]
