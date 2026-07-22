import importlib.resources
from functools import lru_cache

from symspellpy import SymSpell, Verbosity

from app.nlp.text_utils import Token, tokenize

# symspellpy only, no fallback library (Block 8 decision) — the
# Windows machine's system RAM is the documented bottleneck (8GB,
# Hardware Distribution), and a second full spelling dictionary
# permanently resident isn't worth it. Domain vocabulary is already
# largely shielded from this step by Step 2's protection scan, so this
# mostly (not exclusively — that scan has imperfect coverage by its
# own documented design) sees plain English typos, symspellpy's core
# case.
MAX_EDIT_DISTANCE = 2


@lru_cache(maxsize=1)
def get_symspell() -> SymSpell:
    sym_spell = SymSpell(max_dictionary_edit_distance=MAX_EDIT_DISTANCE)
    dictionary_path = importlib.resources.files("symspellpy") / "frequency_dictionary_en_82_765.txt"
    sym_spell.load_dictionary(str(dictionary_path), term_index=0, count_index=1)
    return sym_spell


def _is_protected(token: Token, protected_spans: list[tuple[int, int]]) -> bool:
    return any(token.start >= start and token.end <= end for start, end in protected_spans)


def correct_spelling(text: str, protected_spans: list[tuple[int, int]]) -> str:
    """Step 3 — general English spelling correction, skipping any span
    Step 2 marked as protected domain vocabulary.

    Fail-open: a word symspellpy has no confident suggestion for is
    passed through unchanged — same pattern as Block 12's
    exercise-template validation, not a new convention.
    """
    sym_spell = get_symspell()
    tokens = tokenize(text)
    if not tokens:
        return text

    pieces: list[str] = []
    last_end = 0
    for token in tokens:
        pieces.append(text[last_end : token.start])
        if _is_protected(token, protected_spans):
            pieces.append(token.text)
        else:
            suggestions = sym_spell.lookup(
                token.text.lower(), Verbosity.CLOSEST, max_edit_distance=MAX_EDIT_DISTANCE
            )
            pieces.append(suggestions[0].term if suggestions else token.text)
        last_end = token.end
    pieces.append(text[last_end:])
    return "".join(pieces)
