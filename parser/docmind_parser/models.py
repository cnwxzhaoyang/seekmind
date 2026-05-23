from __future__ import annotations

from dataclasses import dataclass, asdict
from typing import Any, Dict, List, Optional


@dataclass
class ParserOptions:
    include_chunks: bool = True
    max_chunk_chars: int = 800
    max_chunks: Optional[int] = None


@dataclass
class ParserRequest:
    request_id: str
    command: str
    path: str
    options: ParserOptions


@dataclass
class ParsedChunk:
    heading: Optional[str]
    page_no: Optional[int]
    text: str
    order: int

    def to_dict(self) -> Dict[str, Any]:
        return asdict(self)


@dataclass
class ParsedDocument:
    title: Optional[str]
    file_type: str
    content: str
    chunks: List[ParsedChunk]

    def to_dict(self) -> Dict[str, Any]:
        return {
            "title": self.title,
            "file_type": self.file_type,
            "content": self.content,
            "chunks": [chunk.to_dict() for chunk in self.chunks],
        }


@dataclass
class ParserError:
    code: str
    message: str
    details: Optional[str] = None

    def to_dict(self) -> Dict[str, Any]:
        return asdict(self)


def request_from_dict(data: Dict[str, Any]) -> ParserRequest:
    options = data.get("options") or {}
    return ParserRequest(
        request_id=str(data.get("request_id", "")),
        command=str(data.get("command", "")),
        path=str(data.get("path", "")),
        options=ParserOptions(
            include_chunks=bool(options.get("include_chunks", True)),
            max_chunk_chars=int(options.get("max_chunk_chars", 800)),
            max_chunks=options.get("max_chunks"),
        ),
    )

