from dataclasses import dataclass
from functools import lru_cache

import spacy


@dataclass
class NlpResult:
    lemmas: list[str]
    keywords: list[str]
    is_on_topic: bool


@lru_cache(maxsize=1)
def get_nlp() -> spacy.language.Language:
    return spacy.load("en_core_web_sm")


def analyze(text: str, matched_concepts: list[str]) -> NlpResult:
    """Step 5 — spaCy tokenize/lemmatize/keyword/stopword extraction.

    Topic-relevance heuristic: is_on_topic is True iff Step 4 (fuzzy
    domain-vocabulary match, run just before this) matched at least one
    known term. Flagging this rather than hiding it — a richer signal
    (e.g. embedding similarity against ChromaDB's knowledge_global)
    would be more accurate, but that's Milestone 9, not built yet. This
    keeps Step 5 CPU-only/no-model-call as Rule 33 requires, at the
    cost of being a coarser heuristic than the long-term design likely
    wants. (Confirmed empirically: "matrix ops" vs "operations on a
    matrix" — see fuzzy_match.py — is exactly this gap in practice, a
    word-choice/synonym mismatch rapidfuzz can't bridge and this
    heuristic doesn't try to.)

    langdetect (real language ID, since en_core_web_sm's own doc.lang_
    doesn't do this) was tried and removed — live-tested unreliable on
    short strings (e.g. "quiz me on eigenvalues" misdetected as French)
    and fed nothing downstream. Language detection is not implemented
    here; PRD.md's "English only" requirement is currently unenforced
    at this layer.
    """
    doc = get_nlp()(text)
    lemmas = [tok.lemma_.lower() for tok in doc if tok.is_alpha]
    keywords = [tok.lemma_.lower() for tok in doc if tok.is_alpha and not tok.is_stop]
    return NlpResult(
        lemmas=lemmas,
        keywords=keywords,
        is_on_topic=bool(matched_concepts),
    )
