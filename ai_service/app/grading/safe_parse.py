import re

import asteval
import sympy

# Digit directly before a letter/"(" -> insert "*" ("2x"->"2*x",
# "2sin(x)"->"2*sin(x)", "2(x+1)"->"2*(x+1)"). ")" directly before a
# letter/digit/"(" -> insert "*" ("(x+1)(x-1)"->"(x+1)*(x-1)"). Neither
# rule ever inserts "*" between two letters, so a function name
# immediately followed by "(" (e.g. "sin(x)") is never touched — this
# is plain text substitution, no code execution, safe to run before
# asteval ever sees the string.
#
# Deliberately NOT handling a bare variable directly followed by "("
# (e.g. "x(x+1)") as implicit multiplication — genuinely ambiguous
# with an attempted (undefined) function call, and asteval already
# fails that gracefully via safe_parse_symbolic's caller. A narrower,
# unambiguous scope beats guessing.
_IMPLICIT_MULT_DIGIT = re.compile(r"(\d)(?=[a-zA-Z(])")
_IMPLICIT_MULT_PAREN = re.compile(r"(\))(?=[a-zA-Z0-9(])")


def _normalize_math_notation(text: str) -> str:
    """PRD.md, Assessment Engine: "normalize ('2x'->'2*x' etc)".
    asteval is a strict Python-syntax evaluator (same as Python itself)
    with no implicit-multiplication or "^"-as-power support — unlike
    SymPy's own parse_expr, which handled this via its
    implicit_multiplication_application transformation. Confirmed live
    this was a real regression once asteval replaced parse_expr for
    security: "5x" failed to grade correctly until this normalization
    was added back, text-only, ahead of the safe evaluator.

    Tried reusing SymPy's own stringify_expr (tokenize-level rewrite,
    no eval) instead of a custom regex first — rejected: it lowers
    everything into Symbol(...)/Integer(...) constructor-call strings
    for its OWN eval step, and without a populated function namespace
    it broke "sin(x)" into three one-letter symbols multiplied
    together. A small, purpose-built regex is more contained and
    auditable for the specific, narrow thing actually needed here.
    """
    text = text.replace("^", "**")
    text = _IMPLICIT_MULT_DIGIT.sub(r"\1*", text)
    text = _IMPLICIT_MULT_PAREN.sub(r"\1*", text)
    return text

# Only what grading actually requires, nothing broader — confirmed live
# that asteval's own default symtable (387 entries) AND its
# minimal=True symtable (124 entries) both still include risky
# builtins irrelevant to math grading (open, dir, id, hash, type,
# format, repr — open specifically is a real file-access risk, not
# just noise). Passing an explicit symtable= to Interpreter() fully
# REPLACES asteval's own table rather than merging with it (verified:
# a custom {"x": 42} symtable left only "x" and asteval's own harmless
# internal print binding) — so this never touches asteval's defaults
# at all.
ALLOWED_FUNCTIONS: dict = {
    "sin": sympy.sin,
    "cos": sympy.cos,
    "tan": sympy.tan,
    "sqrt": sympy.sqrt,
    "log": sympy.log,
    "exp": sympy.exp,
    "Abs": sympy.Abs,
    "factorial": sympy.factorial,
}


class UnsafeExpressionError(Exception):
    """student_answer failed safe parsing — could be a genuine
    malformed answer or a rejected/disallowed construct. Either way,
    the caller's job is to ask the student to reformat, not crash."""


def safe_parse_symbolic(text: str, reference_expr: sympy.Basic) -> sympy.Basic:
    """Parses untrusted student_answer into a SymPy expression via an
    AST-allowlist evaluator (asteval), never SymPy's own parse_expr()/
    sympify() on raw student text — those use Python's eval()
    internally and are explicitly documented by SymPy itself as unsafe
    on unsanitized input. Confirmed exploitable, not just theoretical:
    parse_expr("lambda x: x") returns a real Lambda object rather than
    a parse error.

    reference_expr — the exercise's correct_answer, trusted/Claude-
    authored content, safe to sympify() directly elsewhere — supplies
    the only variable names this symbol table will recognize.
    student_answer can only reference symbols that legitimately belong
    to this exercise; anything else is an undefined name to asteval.

    Verified against the already-found bypass (lambda) and an
    attribute-access/introspection-chain attempt
    ("(1).__class__.__bases__", "x.__class__") — both rejected cleanly
    by asteval itself (NotImplementedError / AttributeError on unsafe
    attribute access), not by anything added here.
    """
    free_symbols = {str(s): s for s in reference_expr.free_symbols}
    symtable = {**free_symbols, **ALLOWED_FUNCTIONS}

    normalized_text = _normalize_math_notation(text)
    interpreter = asteval.Interpreter(symtable=symtable, minimal=True, use_numpy=False)
    result = interpreter.eval(normalized_text, raise_errors=False, show_errors=False)

    if interpreter.error:
        detail = "; ".join(str(err.get_error()) for err in interpreter.error)
        raise UnsafeExpressionError(detail)

    return result
