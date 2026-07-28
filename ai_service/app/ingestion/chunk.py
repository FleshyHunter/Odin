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

    return chunks
