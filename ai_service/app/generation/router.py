import json

from fastapi import APIRouter
from fastapi.responses import StreamingResponse
from pydantic import BaseModel

from app.generation.service import generate_text, generate_text_stream, generate_text_stream_with_tools

router = APIRouter()

# The tutor's real system prompt — persona, precision, and math-format
# instructions, plus English-only enforcement (PRD.md, RULES.md — locked
# requirement; deferred.md #14, previously unenforced after langdetect
# was tried and removed for misreading short strings). Folded into these
# two endpoints specifically — NOT into generate_text()/generate_text_
# stream() themselves — so intent/classifier.py's own direct qwen-
# fallback call (which bypasses this router entirely) is unaffected;
# only real chat-turn responses are gated/framed. Before this, the ONLY
# system prompt ever sent anywhere in this app was the English-only
# clause alone — no persona, no precision instruction, no format
# guidance at all. Found live: a cold, context-free "generate a matrix,
# do RREF, find eigenvalues" test misread RREF as "reference" and
# eigenvalues as "even values" — not a model-capability ceiling so much
# as zero framing to work with. The LaTeX instruction pairs with the
# frontend's new KaTeX rendering (MessageBubble.tsx) — qwen already
# reaches for LaTeX notation unprompted, this just confirms the format
# and gives it a stated convention (inline vs. block) rather than
# leaving it to guess.
_SYSTEM_PROMPT = (
    "You are a patient, encouraging tutor. Explain the reasoning, not just the "
    "answer — help the student actually understand, not just get through the "
    "material.\n\n"
    "Be precise with technical terminology. If an abbreviation or term is "
    "genuinely ambiguous, ask what the student means rather than guessing — "
    "never silently reinterpret it into something else.\n\n"
    "Use LaTeX for math notation: $...$ inline, $$...$$ for block equations, "
    "matrices, and expressions.\n\n"
    "When you illustrate or demonstrate something — a worked example, a "
    "step-by-step derivation, a concrete instance of a formula — set it "
    "apart in a fenced code block (```...```), separate from the prose "
    "explaining it. This is distinct from the LaTeX instruction above: "
    "LaTeX is for math notation itself; this is for visually separating "
    "'I'm explaining' from 'I'm now showing you the worked step.'\n\n"
    "If the student's message is not written in English, do not answer "
    "their question — respond only with: \"I can only help in English "
    "right now — please rephrase your question in English.\" Otherwise, "
    "answer normally.\n\n"
    "When the prompt includes a <reference_material> block, judge "
    "language ONLY from the text inside <student_message> tags — never "
    "from <reference_material>. Reference material may be in any "
    "language, or contain symbols, code, or non-English text; that is "
    "never a reason to refuse."
)


class HistoryMessage(BaseModel):
    # "user" | "assistant" — Ollama /api/chat's own role names, already
    # mapped by the caller (backend/src/memoryless/turn.rs) from the
    # messages table's "user"/"tutor" role strings. deferred.md #54.
    role: str
    content: str


class GenerateRequest(BaseModel):
    prompt: str
    think: bool = True
    # Only /generate/stream's handler below actually reads this — kept
    # on the one shared request model rather than forked into a second
    # one, matching how prompt/think are already shared between both
    # routes even though /generate (blocking) has never needed history.
    history: list[HistoryMessage] = []


class GenerateResponse(BaseModel):
    response: str


@router.post("/generate", response_model=GenerateResponse)
def generate(request: GenerateRequest) -> GenerateResponse:
    return GenerateResponse(
        response=generate_text(request.prompt, request.think, system=_SYSTEM_PROMPT)
    )


# NDJSON (one JSON object per line), not SSE — this is the FastAPI->Rust
# hop only (Rule 9: Rust is the sole AI-gateway boundary), and NDJSON is
# simpler to parse there than SSE's framing. Rust re-exposes this to the
# browser as real SSE separately (backend/src/memoryless), which is
# where a proper Server-Sent-Events format actually earns its keep.
#
# Two possible lines: {"delta": "..."} for each text chunk, or exactly
# one {"error": "..."} as the LAST line if generation fails partway —
# the HTTP status is already 200 by the time any of this is streaming,
# so a status code can't carry a mid-stream failure; this is the only
# way to signal one to the caller. Shared by both streaming endpoints
# below — takes an already-built delta iterator so each endpoint's own
# generator-construction (which service function, which args) stays
# separate and explicit rather than this helper needing to know about
# tools/history plumbing itself.
def _wrap_ndjson(deltas):
    try:
        for delta in deltas:
            yield json.dumps({"delta": delta}) + "\n"
    except Exception as exc:  # noqa: BLE001 - deliberately broad: whatever
        # Ollama/the client raises here must still reach the caller as a
        # clean final line, not a bare disconnected stream.
        yield json.dumps({"error": str(exc)}) + "\n"


@router.post("/generate/stream")
def generate_stream(request: GenerateRequest) -> StreamingResponse:
    history_dicts = [{"role": message.role, "content": message.content} for message in request.history]
    deltas = generate_text_stream(request.prompt, request.think, system=_SYSTEM_PROMPT, history=history_dicts)
    return StreamingResponse(_wrap_ndjson(deltas), media_type="application/x-ndjson")


# deferred.md #93 — genuinely separate from /generate/stream above, not
# a shared `tools: bool` flag on GenerateRequest: some real callers
# (e.g. journeys/turn.rs's branch-confirmation acknowledgment) want a
# short, deterministic reply and should never have a search tool
# available to reach for. "Capability before caller" — no Rust call
# site wired to this yet; built and real-model-verified
# (generation/service.py's own comment on generate_text_stream_with_
# tools has the verification detail) so it's ready when that wiring
# happens.
@router.post("/generate/stream_with_tools")
def generate_stream_with_tools(request: GenerateRequest) -> StreamingResponse:
    history_dicts = [{"role": message.role, "content": message.content} for message in request.history]
    deltas = generate_text_stream_with_tools(
        request.prompt, request.think, system=_SYSTEM_PROMPT, history=history_dicts
    )
    return StreamingResponse(_wrap_ndjson(deltas), media_type="application/x-ndjson")
