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
    from docmind_parser.parser import ParserException, parse_document
else:
    from .models import ParserError, request_from_dict
    from .parser import ParserException, parse_document


def main() -> int:
    try:
        raw = sys.stdin.read()
        if not raw.strip():
            return write_error(
                request_id="",
                error=ParserError(
                    code="INVALID_REQUEST",
                    message="empty request payload",
                ),
            )

        payload = json.loads(raw)
        request = request_from_dict(payload)

        if request.command != "parse_document":
            return write_error(
                request_id=request.request_id,
                error=ParserError(
                    code="INVALID_REQUEST",
                    message=f"unsupported command: {request.command}",
                ),
            )

        document = parse_document(Path(request.path), request.options)
        response = {
            "request_id": request.request_id,
            "ok": True,
            "document": document.to_dict(),
            "error": None,
        }
        sys.stdout.write(json.dumps(response, ensure_ascii=False))
        sys.stdout.flush()
        return 0
    except ParserException as exc:
        return write_error(request_id=extract_request_id(raw if "raw" in locals() else ""), error=exc.error)
    except FileNotFoundError as exc:
        return write_error(
            request_id=extract_request_id(raw if "raw" in locals() else ""),
            error=ParserError(code="FILE_NOT_FOUND", message=str(exc)),
        )
    except zipfile.BadZipFile as exc:
        return write_error(
            request_id=extract_request_id(raw if "raw" in locals() else ""),
            error=ParserError(code="PARSE_FAILED", message="invalid docx archive", details=str(exc)),
        )
    except Exception as exc:  # noqa: BLE001
        return write_error(
            request_id=extract_request_id(raw if "raw" in locals() else ""),
            error=ParserError(code="INTERNAL_ERROR", message=str(exc)),
        )


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
