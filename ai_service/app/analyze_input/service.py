import json
import logging

from pydantic import ValidationError

from app.acquisition.dify_client import DifyError
from app.acquisition.service import classify_gap
from app.embedding.service import embed_texts
from app.intent.classifier import classify_intent
from app.knowledge.service import GapBand, classify_topic_gap, is_reply_on_topic_for_concept
from app.nlp.fuzzy_match import correct_known_terms, get_protected_spans
from app.nlp.shorthand import expand_shorthand
from app.nlp.spacy_pipe import analyze as spacy_analyze
from app.nlp.spellcheck import correct_spelling

logger = logging.getLogger(__name__)


async def analyze_input(
    text: str,
    known_terms: list[str],
    current_concept_id: str | None = None,
    subject_id: str | None = None,
    dag_version: int | None = None,
    subject_title: str | None = None,
    current_concept_title: str | None = None,
) -> dict:
    """Thin orchestrator — sequences the 6-step pipeline in the exact
    order ARCHITECTURE.md/RULES.md rule 33 specify. All the actual logic
    lives in nlp/ and intent/; this just wires steps together and shapes
    the response.

    known_terms comes from Rust, not a local DB query — ai_service has
    zero Postgres dependencies (confirmed: no driver in
    requirements.txt), so canonical_concepts/concept_aliases vocabulary
    has to be supplied by the caller. Subject-scoped: the current
    journey's subject's concepts only, not the full cross-subject
    vocabulary bank (Tangent Mode's global-KB search is the one
    explicit exception to subject-scoping; this isn't it).

    current_concept_id: the concept the student is currently on, if
    mid-journey (nullable). Only used to decide TANGENT vs OUT_OF_SCOPE
    when is_on_topic is False — see classify_intent.

    subject_id/dag_version (deferred.md #75/2b): only meaningful
    alongside current_concept_id — together they're enough to look up
    that concept's own stored embedding (concept_embeddings, id format
    concept_id:subject_id:dag_version) for the richer on-topic check
    below. All three are None for memoryless mode (no subject exists to
    look anything up in), same as current_concept_id always was.

    subject_title/current_concept_title (deferred.md #2b): display
    names, only used to build Stage 2's classify_gap() prompt if it
    fires — nothing else here needs them. None-able independently of
    subject_id/dag_version (a caller could plausibly have the IDs but
    not fetch titles) — Stage 2 is just skipped, staying AMBIGUOUS,
    rather than treating a missing title as an error.
    """
    step1_text = expand_shorthand(text)

    protected_spans = get_protected_spans(step1_text, known_terms)
    step3_text = correct_spelling(step1_text, protected_spans)

    step4_text, matched_concepts = correct_known_terms(step3_text, known_terms)

    nlp_result = spacy_analyze(step4_text, matched_concepts)

    # deferred.md #75/2b: Step 5's own word-match is coarse by its own
    # admission (spacy_pipe.py's docstring) — a reply can be completely
    # on-topic without containing any concept-title vocabulary (found
    # live: "The red ones have more, that is 3 vs 2" scored
    # is_on_topic=False against "which group has more"). When word-match
    # alone says False AND there's a real concept to compare against,
    # a second, richer signal gets a chance to override it to True —
    # never the reverse: an actual word match is never second-guessed,
    # and this whole block is skipped entirely outside a journey
    # (current_concept_id/subject_id/dag_version all None), so
    # memoryless mode's behavior is completely unaffected.
    is_on_topic = nlp_result.is_on_topic
    # deferred.md #2b — gap_classification is a SEPARATE, additive
    # signal, not fed into classify_intent below: 2c/2d (the actual
    # consumers of "dag_gap" vs "off_topic") don't exist yet, so
    # detected_intent stays exactly what it always was (TANGENT for
    # either sub-case) until there's real behavior to route to.
    gap_classification: str | None = None
    if not is_on_topic and current_concept_id and subject_id and dag_version is not None:
        [reply_embedding] = embed_texts([step4_text])
        is_on_topic = is_reply_on_topic_for_concept(reply_embedding, current_concept_id, subject_id, dag_version)

        if not is_on_topic:
            # Stage 1 (cheap, deterministic) — broadens the same
            # on-topic question from ONE concept (just above) to the
            # whole subject's DAG.
            gap_signal = classify_topic_gap(reply_embedding, subject_id, dag_version)
            if gap_signal.band == GapBand.ON_TOPIC_ELSEWHERE:
                is_on_topic = True
            else:
                gap_classification = gap_signal.band.value
                if gap_signal.band == GapBand.AMBIGUOUS and subject_title and current_concept_title:
                    # Stage 2 (expensive, LLM) — only for the band
                    # Stage 1 couldn't resolve on vector math alone.
                    # Fails soft: Dify isn't configured in every
                    # environment (DifyNotConfigured is a DifyError
                    # subclass), and even a genuine failure just means
                    # "stays ambiguous" — no caller acts on off_topic
                    # vs dag_gap differently yet, so nothing behavioral
                    # is lost either way.
                    try:
                        gap_result = await classify_gap(
                            subject_title, known_terms, current_concept_title, step4_text
                        )
                        gap_classification = gap_result.classification
                    except (DifyError, ValidationError, json.JSONDecodeError, TypeError) as err:
                        logger.info("classify_gap: Stage 2 unavailable, staying ambiguous: %s", err)

    intent = classify_intent(step4_text, is_on_topic, current_concept_id)

    return {
        "raw_input": text,
        "cleaned_query": step4_text,
        "lemmas": nlp_result.lemmas,
        "keywords": nlp_result.keywords,
        "is_on_topic": is_on_topic,
        "matched_concepts": matched_concepts,
        "detected_intent": intent.value,
        "gap_classification": gap_classification,
    }
