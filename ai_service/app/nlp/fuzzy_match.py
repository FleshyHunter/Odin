from dataclasses import dataclass

from rapidfuzz import fuzz, process

from app.nlp.spacy_pipe import get_nlp

DEFAULT_THRESHOLD = 80.0


@dataclass
class VocabMatch:
    start: int
    end: int
    original: str
    matched_term: str
    score: float


def _single_word_terms(vocabulary: list[str]) -> list[str]:
    return [t for t in vocabulary if len(t.split()) == 1]


def _multi_word_terms(vocabulary: list[str]) -> list[str]:
    return [t for t in vocabulary if len(t.split()) > 1]


def _lemmatize_phrase(nlp, phrase: str) -> str:
    return " ".join(tok.lemma_.lower() for tok in nlp(phrase) if tok.is_alpha)


def match_vocabulary(
    text: str, vocabulary: list[str], threshold: float = DEFAULT_THRESHOLD
) -> list[VocabMatch]:
    """Core single-word fuzzy-matching logic (rapidfuzz, edit-distance on
    LEMMAS) — the SAME function backing both Step 2 and Step 4
    (ARCHITECTURE.md, Input Normalization): "the domain vocabulary check
    now runs twice... Reuses the same cheap, already-free function for
    both jobs — no new dependency, just called at two points in the
    pipeline." This function only FINDS matches; callers (below) decide
    what to do with them.

    Compares each input token's LEMMA against each single-word
    vocabulary term's LEMMA (both via spaCy, already in the stack for
    Step 5 — reused here for a second purpose, no new dependency) —
    this is what lets "matrices" match a "matrix" vocabulary entry
    (verified live: rapidfuzz.fuzz.ratio alone scores that pair 71.4,
    below threshold; lemma-to-lemma comparison scores it as an exact
    match instead). Positions/spans still come from spaCy's own
    tokenization (used for both position AND lemma here, rather than
    maintaining two separate tokenizers' worth of boundaries).

    Multi-word vocabulary entries are NOT handled by this function —
    see match_multiword_vocabulary below. Known limitation, stated
    rather than silently assumed: multi-word matches don't have a safe
    contiguous span in the general case (a phrase's words can appear
    reordered/scattered), so they're detected but never used to rewrite
    text — only this single-word path does text substitution.
    """
    single_word_terms = _single_word_terms(vocabulary)
    if not single_word_terms:
        return []

    nlp = get_nlp()
    lemmatized_terms = [_lemmatize_phrase(nlp, term) or term.lower() for term in single_word_terms]

    matches: list[VocabMatch] = []
    for tok in nlp(text):
        if not tok.is_alpha:
            continue
        result = process.extractOne(
            tok.lemma_.lower(), lemmatized_terms, scorer=fuzz.ratio, score_cutoff=threshold
        )
        if result is not None:
            _, score, idx = result
            canonical_term = single_word_terms[idx]
            matches.append(VocabMatch(tok.idx, tok.idx + len(tok.text), tok.text, canonical_term, score))
    return matches


def match_multiword_vocabulary(
    text: str, vocabulary: list[str], threshold: float = DEFAULT_THRESHOLD
) -> list[tuple[str, float]]:
    """Multi-word vocabulary matching (Step 4 addition) — e.g. "vector
    spaces", "matrix ops" (real examples: canonical_concepts.title is
    free-form LLM-generated text with no single-word constraint, and
    the Roadmap mockup's own sample data already uses multi-word
    titles).

    Approach: rapidfuzz's token_set_ratio comparing each multi-word
    term against the WHOLE cleaned input text (lemmatized on both
    sides) — no sliding window. token_set_ratio is built for exactly
    this: it tolerates word reordering and extra surrounding words,
    so a term embedded anywhere in a longer sentence still scores well
    regardless of exact phrasing position. Verified live before
    building this:
      "vector spaces" vs "vector space"                    -> 96.0
      "vector spaces" vs "what are vector spaces"           -> 100.0
      "vector spaces" vs "space of vectors" (reordered)     -> 82.8
      "vector spaces" vs "can you explain vector spaces..." -> 100.0
    All clear DEFAULT_THRESHOLD (80.0).

    Known, NOT reliably fixed by lemmatization: word-choice/synonym
    gaps, e.g. "matrix ops" vs "operations on a matrix". Before
    lemmatization was added this scored 75.0 (a clean non-match).
    Lemmatizing both sides ("ops"->"op", "operations"->"operation")
    shifts it to exactly 80.0 — landing ON the cutoff, not clearly
    below it. Verified live; not a deliberate design outcome, an
    accidental side effect of shortening both word forms by a
    character. Threshold was NOT tuned to make this pass or fail either
    way — 80.0 is unchanged from the single-word matcher. This pair is
    now a genuine boundary case (a 1-character difference either
    direction could flip it), which is a materially different, more
    honest story than "confirmed non-match" — flagging that distinction
    explicitly rather than re-asserting the earlier claim. It's still
    the SAME underlying gap already named where is_on_topic's heuristic
    is flagged as coarse pending real semantic matching (Milestone
    9/ChromaDB) — a synonym/word-choice mismatch, not something rapidfuzz
    (lemmatized or not) reliably bridges either way.

    Detection-only: unlike match_vocabulary, this never rewrites
    cleaned_query (no safe contiguous span to substitute — token_set_ratio
    doesn't require the term's words to be contiguous or in order). It
    only contributes to matched_concepts.
    """
    multi_word_terms = _multi_word_terms(vocabulary)
    if not multi_word_terms:
        return []

    nlp = get_nlp()
    lemmatized_text = _lemmatize_phrase(nlp, text)
    if not lemmatized_text:
        return []

    matches: list[tuple[str, float]] = []
    for term, term_doc in zip(multi_word_terms, nlp.pipe(multi_word_terms)):
        lemmatized_term = " ".join(tok.lemma_.lower() for tok in term_doc if tok.is_alpha)
        score = fuzz.token_set_ratio(lemmatized_term, lemmatized_text)
        if score >= threshold:
            matches.append((term, score))
    return matches


def get_protected_spans(
    text: str, vocabulary: list[str], threshold: float = DEFAULT_THRESHOLD
) -> list[tuple[int, int]]:
    """Step 2 — pre-spellcheck domain protection scan. Marks spans
    spellcheck (Step 3) must skip entirely rather than "correcting."

    Single-word terms only (same scope as match_vocabulary) — a
    multi-word phrase's individual words are real, already-correctly-
    spelled English words in practice (e.g. "vector", "spaces"), so
    Step 3 is very unlikely to touch them anyway; not adding
    multi-word span-protection here to avoid the same span-safety
    problem match_multiword_vocabulary's docstring names.
    """
    return [(m.start, m.end) for m in match_vocabulary(text, vocabulary, threshold)]


def correct_known_terms(
    text: str, vocabulary: list[str], threshold: float = DEFAULT_THRESHOLD
) -> tuple[str, list[str]]:
    """Step 4 — main correction pass (runs on the already spell-checked
    text) and redundant safety net for anything Step 2 missed. Returns
    the corrected text AND the list of matched canonical terms, since
    the orchestrator needs the latter for the response's
    matched_concepts field.

    Runs BOTH single-word matching (match_vocabulary — rewrites text)
    and multi-word matching (match_multiword_vocabulary — detection
    only, see its docstring) and unions their matched terms.
    """
    matches = match_vocabulary(text, vocabulary, threshold)
    multiword_matches = match_multiword_vocabulary(text, vocabulary, threshold)

    if not matches:
        corrected_text = text
    else:
        pieces: list[str] = []
        last_end = 0
        for m in matches:
            pieces.append(text[last_end : m.start])
            pieces.append(m.matched_term)
            last_end = m.end
        pieces.append(text[last_end:])
        corrected_text = "".join(pieces)

    matched_terms = [m.matched_term for m in matches] + [term for term, _score in multiword_matches]
    return corrected_text, matched_terms
