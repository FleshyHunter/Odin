// A canonical `exercises` row (or a fresh `generate_dag()` diagnostic_
// primary/backup) is a TEMPLATE: `template_body.question_template`/
// `answer_template`/`choices_template` carry `{name}` placeholders,
// `template_params` carries a `{"min": x, "max": y}` range per name
// (ai_service/app/acquisition/service.py's own `_instantiate_params`/
// `_sanity_check_instantiation` establish this exact syntax — see that
// file's comments). Those two functions only ever VALIDATE that a
// template CAN be filled at generation time; nothing on the Rust side
// has ever actually filled one in for real presentation/grading. The
// Onboarding Diagnostic (deferred.md #4) is the first caller that needs
// a concrete, gradeable question, so this is a minimal, diagnostic-
// scoped port of that same fill logic — not a shared/generic templating
// engine, since nothing else needs one yet.

use std::collections::HashMap;

use rand::RngExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ai_client::ExerciseTemplate;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstantiatedExercise {
    pub exercise_type: String,
    pub difficulty: String,
    pub rendered_question: String,
    pub choices: Option<Vec<String>>,
    pub instantiated_params: Value,
    pub correct_answer: Option<String>,
    pub grader_type: Option<String>,
    pub grader_config: Option<Value>,
    pub tolerance: Option<f32>,
}

pub fn instantiate(template: &ExerciseTemplate) -> InstantiatedExercise {
    let params = draw_params(template.template_params.as_ref());

    let question_template = template
        .template_body
        .get("question_template")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let rendered_question = fill(question_template, &params);

    let correct_answer = template.correct_answer.as_deref().map(|raw| fill(raw, &params));

    let choices = template
        .template_body
        .get("choices_template")
        .and_then(Value::as_array)
        .map(|choices| {
            choices
                .iter()
                .filter_map(Value::as_str)
                .map(|choice| fill(choice, &params))
                .collect()
        });

    let instantiated_params = serde_json::to_value(&params).unwrap_or(Value::Null);

    InstantiatedExercise {
        exercise_type: template.exercise_type.clone(),
        difficulty: template.difficulty.clone(),
        rendered_question,
        choices,
        instantiated_params,
        correct_answer,
        grader_type: template.grader_type.clone(),
        grader_config: template.grader_config.clone(),
        tolerance: template.tolerance,
    }
}

/// Draws ONE random value per `{name}: {min, max}` entry — mirrors
/// `_instantiate_params`'s own int-vs-float branch (a range where both
/// bounds are whole numbers draws a whole number, matching what a "how
/// many X" question expects; anything else draws a float).
fn draw_params(template_params: Option<&Value>) -> HashMap<String, f64> {
    let mut values = HashMap::new();
    let Some(Value::Object(map)) = template_params else {
        return values;
    };
    for (name, spec) in map {
        let (Some(lo), Some(hi)) = (spec.get("min"), spec.get("max")) else {
            continue;
        };
        let value = match (lo.as_i64(), hi.as_i64()) {
            (Some(lo), Some(hi)) if hi >= lo => rand::rng().random_range(lo..=hi) as f64,
            _ => {
                let lo = lo.as_f64().unwrap_or(0.0);
                let hi = hi.as_f64().unwrap_or(lo);
                if hi > lo {
                    rand::rng().random_range(lo..hi)
                } else {
                    lo
                }
            }
        };
        values.insert(name.clone(), value);
    }
    values
}

/// Whole-number draws render without a trailing ".0" ("Solve 3x + 5 =
/// 20", not "3.0x + 5.0 = 20.0") — matches how Claude's own templates
/// read.
fn fill(template: &str, params: &HashMap<String, f64>) -> String {
    let mut out = template.to_string();
    for (name, value) in params {
        let placeholder = format!("{{{name}}}");
        let rendered = if value.fract() == 0.0 {
            format!("{}", *value as i64)
        } else {
            format!("{value}")
        };
        out = out.replace(&placeholder, &rendered);
    }
    out
}
