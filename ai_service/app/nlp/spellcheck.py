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


def _is_numeric(token: Token) -> bool:
    # deferred.md #52: symspellpy's dictionary is English words only —
    # a digit-only token like "7" has no real entry, but at
    # MAX_EDIT_DISTANCE=2 a single-character substitution to a common
    # short word ("a") is well within range, so lookup() "corrects" it
    # instead of having no suggestion. tokenize()'s regex ([A-Za-z0-9']+)
    # only ever produces a pure-digit token for genuinely numeric input
    # (e.g. "3.14" splits into "3"/"14" since "." isn't in the class) —
    # isdigit() alone is enough, no need to handle decimals/signs here.
    return token.text.isdigit()


def _is_acronym(token: Token) -> bool:
    # Same failure mode as #52, different token shape: an unrecognized
    # ALL-CAPS token (SSSP, DFS, CPU...) has no entry in symspellpy's
    # lowercase-English-word dictionary either, so at MAX_EDIT_DISTANCE=2
    # it gets silently "corrected" to the nearest real word instead of
    # left alone — confirmed live: "SSSP" -> "shop". Genuine typos are
    # essentially never all-caps, so this is a safe, narrow heuristic —
    # not a replacement for Step 2's vocabulary-based protection
    # (get_protected_spans), which only covers terms already in a
    # specific subject's known_terms and does nothing in memoryless
    # mode (empty vocabulary) or for domain acronyms no subject has
    # ever declared. length >= 2 only — a lone capital letter ("I") is
    # both common, correctly-spelled English on its own and genuinely
    # ambiguous as "acronym vs. real word", so it stays correctable.
    #
    # Strips one trailing lowercase "s" before checking — plural
    # acronyms ("APIs", "CPUs", "SSSPs") are common enough in casual
    # phrasing that a strict all-caps check would otherwise miss them
    # (isupper() is False the moment any cased character is lowercase).
    # A genuine acronym never ends in a literal lowercase "s" on its
    # own (that would make it lowercase, not all-caps), so this can't
    # misfire on the acronym itself — only ever strips a real plural.
    core = token.text[:-1] if token.text.endswith("s") else token.text
    return len(core) >= 2 and core.isupper()


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
        if _is_protected(token, protected_spans) or _is_numeric(token) or _is_acronym(token):
            pieces.append(token.text)
        else:
            suggestions = sym_spell.lookup(
                token.text.lower(), Verbosity.CLOSEST, max_edit_distance=MAX_EDIT_DISTANCE
            )
            pieces.append(suggestions[0].term if suggestions else token.text)
        last_end = token.end
    pieces.append(text[last_end:])
    return "".join(pieces)
