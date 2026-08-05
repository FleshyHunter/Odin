import os

from app.nlp.spacy_pipe import get_nlp

# PRD.md, Flow 1: "chunk (300-500 tokens, sentence boundaries)" — locked
# range, env-configurable per Rule 12, not hardcoded.
CHUNK_MIN_TOKENS = int(os.environ.get("CHUNK_MIN_TOKENS", "300"))
CHUNK_MAX_TOKENS = int(os.environ.get("CHUNK_MAX_TOKENS", "500"))


def _token_count(text: str) -> int:
    # Word-count approximation, not the embedding model's own
    # tokenizer — no doc locks an exact counting method, and this is
    # deterministic and fast enough for both the chunk-boundary
    # decision below and the stored token_count value. A judgment
    # call, flagged rather than silently assumed.
    return len(text.split())


def chunk_text(text: str) -> list[str]:
    """Splits extracted text into ~300-500 token chunks, never cutting
    mid-sentence — reuses the same spaCy model Block 8 already loads
    (en_core_web_sm) for sentence segmentation, rather than a second
    NLP dependency just for this.

    A single sentence that alone exceeds CHUNK_MAX_TOKENS still becomes
    its own (oversized) chunk — the `current and` guard below only
    flushes when there's already something to flush, deliberately: a
    sentence can't be split further without breaking the "never cut
    mid-sentence" guarantee, so exceeding the max here is an accepted,
    unavoidable edge case (plausible on OCR'd text with little
    punctuation), not a bug — CHUNK_MIN_TOKENS below is a real, enforced
    floor, but CHUNK_MAX_TOKENS was only ever a soft target once a
    sentence is already this long.
    """
    doc = get_nlp()(text)
    sentences = [sent.text.strip() for sent in doc.sents if sent.text.strip()]

    chunks: list[str] = []
    current: list[str] = []
    current_tokens = 0

    for sentence in sentences:
        sentence_tokens = _token_count(sentence)
        if current and current_tokens + sentence_tokens > CHUNK_MAX_TOKENS:
            chunks.append(" ".join(current))
            current = []
            current_tokens = 0
        current.append(sentence)
        current_tokens += sentence_tokens

    if current:
        chunks.append(" ".join(current))

    return _enforce_min_tokens(chunks)


def _enforce_min_tokens(chunks: list[str]) -> list[str]:
    """CHUNK_MIN_TOKENS floor (previously declared, read from env, and
    never actually enforced anywhere — found by an independent audit,
    2026-08-05). An under-filled chunk isn't only possible as the very
    last chunk of a document: it happens any time the sentence
    immediately following a short chunk alone would have pushed it over
    CHUNK_MAX_TOKENS, forcing an early flush partway through. Carries
    each undersized chunk forward, merging into however many follow
    until the combined total clears the floor, rather than leaving an
    undersized orphan as its own retrieval unit; a single chunk with
    nothing to merge into (the whole document is one short chunk) is
    left as-is — there's no floor a lone chunk could fail to meet in
    any useful sense."""
    if len(chunks) < 2:
        return chunks

    merged: list[str] = []
    carry = ""
    for chunk in chunks:
        combined = f"{carry} {chunk}".strip() if carry else chunk
        if _token_count(combined) < CHUNK_MIN_TOKENS:
            carry = combined
            continue
        merged.append(combined)
        carry = ""

    if carry:
        if merged:
            merged[-1] = f"{merged[-1]} {carry}"
        else:
            merged.append(carry)

    return merged
