"""
@author MorningSun
@CreatedDate 2026/06/03
@Description Python sidecar 入口，分发解析、embedding 与 RAG 流式任务。
"""

from __future__ import annotations

import json
import sys
import zipfile
from pathlib import Path

if __package__ in (None, ""):
    package_root = Path(__file__).resolve().parents[1]
    if str(package_root) not in sys.path:
        sys.path.insert(0, str(package_root))
    from docmind_parser.models import ParserError, request_from_dict
    from docmind_parser.rag.eval import rag_eval_request_from_dict, run_rag_regression
    from docmind_parser.rag.models import rag_request_from_dict
    from docmind_parser.rag.pipeline import run_rag_answer_stream
    from docmind_parser.parser import (
        ParserException,
        embed_texts,
        embedding_status,
        parse_document,
    )
else:
    from .models import ParserError, request_from_dict
    from .rag.eval import rag_eval_request_from_dict, run_rag_regression
    from .rag.models import rag_request_from_dict
    from .rag.pipeline import run_rag_answer_stream
    from .parser import ParserException, embed_texts, embedding_status, parse_document


def main() -> int:
    if len(sys.argv) >= 2 and sys.argv[1] == "warmup-embedding":
        model_name = sys.argv[2] if len(sys.argv) >= 3 else None
        return warmup_embedding(model_name)

    try:
        raw = sys.stdin.read()
        if not raw.strip():
            return write_error(
                request_id="",
                error=ParserError(
                    code="invalid_request",
                    message="empty request payload",
                ),
            )

        payload = json.loads(raw)
        command = str(payload.get("command", ""))

        if command == "rag_answer_stream":
            request = rag_request_from_dict(payload)

            def emit_progress(event: dict) -> None:
                sys.stdout.write(json.dumps(event, ensure_ascii=False))
                sys.stdout.write("\n")
                sys.stdout.flush()

            response = run_rag_answer_stream(request, emit=emit_progress)
            sys.stdout.write(json.dumps(response.to_dict(), ensure_ascii=False))
            sys.stdout.flush()
            return 0 if response.ok else 1

        if command == "rag_eval":
            request, cases = rag_eval_request_from_dict(payload)

            def emit_progress(event: dict) -> None:
                sys.stdout.write(json.dumps(event, ensure_ascii=False))
                sys.stdout.write("\n")
                sys.stdout.flush()

            report = run_rag_regression(request, cases=cases, emit=emit_progress)
            sys.stdout.write(json.dumps(report.to_dict(), ensure_ascii=False))
            sys.stdout.flush()
            return 0 if report.ok else 1

        request = request_from_dict(payload)

        if request.command in {"parse_document", "parse_document_stream"}:
            def emit_progress(payload: dict) -> None:
                sys.stdout.write(json.dumps(payload, ensure_ascii=False))
                sys.stdout.write("\n")
                sys.stdout.flush()

            document = parse_document(
                Path(request.path),
                request.options,
                emit=emit_progress if request.command == "parse_document_stream" else None,
                request_id=request.request_id,
            )
            response = {
                "kind": "response",
                "request_id": request.request_id,
                "ok": True,
                "document": document.to_dict(),
                "vectors": None,
                "embedding_status": None,
                "error": None,
            }
            sys.stdout.write(json.dumps(response, ensure_ascii=False))
            sys.stdout.flush()
            return 0

        if request.command == "embed_texts":
            embedding_result = embed_texts(request.texts, request.model_name)
            response = {
                "kind": "response",
                "request_id": request.request_id,
                "ok": True,
                "document": None,
                "vectors": embedding_result.vectors,
                "embedding_status": embedding_result.status.to_dict(),
                "error": None,
            }
            sys.stdout.write(json.dumps(response, ensure_ascii=False))
            sys.stdout.flush()
            return 0

        if request.command == "embed_texts_stream":
            def emit_progress(payload: dict) -> None:
                sys.stdout.write(json.dumps(payload, ensure_ascii=False))
                sys.stdout.write("\n")
                sys.stdout.flush()

            embedding_result = embed_texts(
                request.texts,
                request.model_name,
                emit=emit_progress,
                request_id=request.request_id,
            )
            response = {
                "kind": "response",
                "request_id": request.request_id,
                "ok": True,
                "document": None,
                "vectors": embedding_result.vectors,
                "embedding_status": embedding_result.status.to_dict(),
                "error": None,
            }
            sys.stdout.write(json.dumps(response, ensure_ascii=False))
            sys.stdout.flush()
            return 0

        if request.command == "embedding_status":
            status = embedding_status(request.model_name)
            response = {
                "kind": "response",
                "request_id": request.request_id,
                "ok": True,
                "document": None,
                "vectors": None,
                "embedding_status": status.to_dict(),
                "error": None,
            }
            sys.stdout.write(json.dumps(response, ensure_ascii=False))
            sys.stdout.flush()
            return 0

        return write_error(
            request_id=request.request_id,
            error=ParserError(
                code="invalid_request",
                message=f"unsupported command: {request.command}",
            ),
        )
    except ParserException as exc:
        return write_error(request_id=extract_request_id(raw if "raw" in locals() else ""), error=exc.error)
    except FileNotFoundError as exc:
        return write_error(
            request_id=extract_request_id(raw if "raw" in locals() else ""),
            error=ParserError(code="file_not_found", message=str(exc)),
        )
    except zipfile.BadZipFile as exc:
        return write_error(
            request_id=extract_request_id(raw if "raw" in locals() else ""),
            error=ParserError(code="parse_failed", message="invalid docx archive", details=str(exc)),
        )
    except Exception as exc:  # noqa: BLE001
        return write_error(
            request_id=extract_request_id(raw if "raw" in locals() else ""),
            error=ParserError(code="internal_error", message=str(exc)),
        )


def warmup_embedding(model_name: str | None) -> int:
    try:
        result = embed_texts(["DocMind embedding warmup"], model_name)
        response = {
            "ok": True,
            "vectors": len(result.vectors),
            "dimension": len(result.vectors[0]) if result.vectors else 0,
            "embedding_status": result.status.to_dict(),
        }
        sys.stdout.write(json.dumps(response, ensure_ascii=False, indent=2))
        sys.stdout.write("\n")
        sys.stdout.flush()
        return 0
    except ParserException as exc:
        sys.stderr.write(json.dumps({"ok": False, "error": exc.error.to_dict()}, ensure_ascii=False, indent=2))
        sys.stderr.write("\n")
        sys.stderr.flush()
        return 1


def extract_request_id(raw: str) -> str:
    if not raw.strip():
        return ""
    try:
        payload = json.loads(raw)
        return str(payload.get("request_id", ""))
    except Exception:  # noqa: BLE001
        return ""


def write_error(request_id: str, error: ParserError) -> int:
    response = {
        "kind": "response",
        "request_id": request_id,
        "ok": False,
        "document": None,
        "error": error.to_dict(),
    }
    sys.stdout.write(json.dumps(response, ensure_ascii=False))
    sys.stdout.flush()
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
