from app.intent.classifier import classify_intent
from app.nlp.fuzzy_match import correct_known_terms, get_protected_spans
from app.nlp.shorthand import expand_shorthand
from app.nlp.spacy_pipe import analyze as spacy_analyze
from app.nlp.spellcheck import correct_spelling


def analyze_input(
    text: str, known_terms: list[str], current_concept_id: str | None = None
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
    """
    step1_text = expand_shorthand(text)

    protected_spans = get_protected_spans(step1_text, known_terms)
    step3_text = correct_spelling(step1_text, protected_spans)

    step4_text, matched_concepts = correct_known_terms(step3_text, known_terms)

    nlp_result = spacy_analyze(step4_text, matched_concepts)
    intent = classify_intent(step4_text, nlp_result.is_on_topic, current_concept_id)

    return {
        "raw_input": text,
        "cleaned_query": step4_text,
        "lemmas": nlp_result.lemmas,
        "keywords": nlp_result.keywords,
        "is_on_topic": nlp_result.is_on_topic,
        "matched_concepts": matched_concepts,
        "detected_intent": intent.value,
    }
