from dataclasses import dataclass

from app.acquisition.service import instantiate_params


class ExerciseInstantiationError(Exception):
    """A canonical exercise's template failed to fill with its own drawn
    values at SERVE time. Templates already passed 5 sample
    instantiations at generation time (acquisition/service.py's
    _sanity_check_instantiation) before ever being stored as canonical —
    this should be rare — but that validation isn't a formal guarantee,
    so a real failure here is a genuine, if unlikely, content bug, not
    a server crash. Caller maps this to a 422, matching deferred.md's
    own audit finding that classify_gap()/fold_concept_into_dag() were
    missing exactly this kind of "malformed content, not our bug"
    -> clean-error-response handling."""


@dataclass
class InstantiatedExercise:
    rendered_question: str
    instantiated_params: dict
    # The REAL per-instance correct answer to grade against. For
    # numeric/symbolic_math (randomized), this is answer_template filled
    # with the SAME drawn values question_template used — grading needs
    # a concrete value, not a template (grading/service.py's grade_exercise
    # takes a plain `correct_answer: str`, never a template string). For
    # types that don't randomize (mcq/fill_blank/short_answer), this is
    # just the exercise's own static correct_answer, unchanged.
    expected_answer: str | None
    # Filled choices, for mcq display — None for every other type.
    rendered_choices: list[str] | None


def instantiate_exercise(
    exercise_type: str,
    template_body: dict,
    template_params: dict | None,
    correct_answer: str | None,
) -> InstantiatedExercise:
    """deferred.md — real serve-time instantiation: draws ONE set of
    random values (instantiate_params, promoted from acquisition/
    service.py's own validation-time helper — same draw, real use this
    time instead of a dry run) and fills question_template/
    answer_template/choices_template with them, exactly the same fill-in
    mechanism acquisition/service.py's own _sanity_check_instantiation
    already exercises 5 times per template at generation time. This is
    the FIRST real caller that keeps what it renders — previous
    instantiations were always throwaway validation runs.
    """
    values = instantiate_params(template_params or {})
    question_template = template_body.get("question_template", "")
    try:
        rendered_question = question_template.format(**values)
    except (KeyError, ValueError) as e:
        raise ExerciseInstantiationError(f"question_template failed to fill with params {values}: {e}") from e

    answer_template = template_body.get("answer_template")
    expected_answer = correct_answer
    if answer_template:
        try:
            expected_answer = answer_template.format(**values)
        except (KeyError, ValueError) as e:
            raise ExerciseInstantiationError(f"answer_template failed to fill with params {values}: {e}") from e

    choices_template = template_body.get("choices_template")
    rendered_choices = None
    if choices_template:
        try:
            rendered_choices = [choice.format(**values) for choice in choices_template]
        except (KeyError, ValueError) as e:
            raise ExerciseInstantiationError(f"choices_template failed to fill with params {values}: {e}") from e

    return InstantiatedExercise(
        rendered_question=rendered_question,
        instantiated_params=values,
        expected_answer=expected_answer,
        rendered_choices=rendered_choices,
    )
