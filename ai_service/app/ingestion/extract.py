import io
import os
from dataclasses import dataclass
from pathlib import Path

import fitz  # PyMuPDF
import pillow_heif
import pytesseract
from docx import Document
from PIL import Image

# Makes PIL.Image.open() transparently handle HEIC/HEIF too — HEIC is
# converted through the SAME Tesseract flow below, not a separate
# pipeline (PRD.md, PNG/Image Uploads). Registered once at import time.
pillow_heif.register_heif_opener()

# No default assumed silently — PRD.md names this exact env var and a
# "default 60%" value, both locked, not invented here.
TESSERACT_CONFIDENCE_THRESHOLD = int(os.environ.get("TESSERACT_CONFIDENCE_THRESHOLD", "60"))

SUPPORTED_SUFFIXES = {".pdf", ".docx", ".txt", ".png", ".heic", ".heif"}


@dataclass
class ExtractResult:
    text: str
    rejected: bool = False
    rejection_reason: str | None = None


def _extract_pdf(data: bytes) -> str:
    document = fitz.open(stream=data, filetype="pdf")
    try:
        return "\n".join(page.get_text() for page in document)
    finally:
        document.close()


def _extract_docx(data: bytes) -> str:
    document = Document(io.BytesIO(data))
    return "\n".join(p.text for p in document.paragraphs)


def _extract_txt(data: bytes) -> str:
    return data.decode("utf-8", errors="replace")


def _extract_image(data: bytes) -> ExtractResult:
    image = Image.open(io.BytesIO(data))

    # pytesseract checks image.format against its OWN hardcoded
    # whitelist before invoking the tesseract binary — that whitelist
    # predates HEIC support and does not include "HEIF" (what
    # pillow-heif reports), so it raises TypeError('Unsupported image
    # format/type') despite PIL having opened the file just fine.
    # Round-tripping through PNG in memory sidesteps this entirely —
    # this IS the HEIC->PNG conversion PRD.md calls for, not a separate
    # workaround.
    if image.format not in pytesseract.pytesseract.SUPPORTED_FORMATS:
        buffer = io.BytesIO()
        image.convert("RGB").save(buffer, format="PNG")
        buffer.seek(0)
        image = Image.open(buffer)

    ocr_data = pytesseract.image_to_data(image, output_type=pytesseract.Output.DICT)
    # pytesseract reports -1 confidence for non-text regions (e.g. pure
    # whitespace blocks) — excluded, not averaged in as a fake zero.
    confidences = [int(c) for c in ocr_data["conf"] if int(c) >= 0]
    avg_confidence = sum(confidences) / len(confidences) if confidences else 0

    if avg_confidence < TESSERACT_CONFIDENCE_THRESHOLD:
        return ExtractResult(
            text="",
            rejected=True,
            rejection_reason="This image is too unclear to read reliably — try a clearer photo or scan.",
        )
    return ExtractResult(text=pytesseract.image_to_string(image))


def extract(data: bytes, filename: str) -> ExtractResult:
    """Extracts raw text from an uploaded file, branching by format.

    Deliberately does NOT compute content_hash, check dedup, or run the
    quality gate — those are the caller's job (Rust computes the hash
    BEFORE this ever runs, per Duplicate Upload Prevention; the quality
    gate runs on the extracted text, after this returns).
    """
    suffix = Path(filename).suffix.lower()

    if suffix == ".pdf":
        return ExtractResult(text=_extract_pdf(data))
    if suffix == ".docx":
        return ExtractResult(text=_extract_docx(data))
    if suffix == ".txt":
        return ExtractResult(text=_extract_txt(data))
    if suffix in (".png", ".heic", ".heif"):
        return _extract_image(data)

    return ExtractResult(
        text="",
        rejected=True,
        rejection_reason=f"Unsupported file type: {suffix or '(none)'}. "
        "Supported: PDF, DOCX, TXT, PNG, HEIC.",
    )
