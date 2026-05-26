from __future__ import annotations

from dataclasses import dataclass, asdict
from typing import Any, Callable, Dict, List, Optional


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
    texts: List[str]
    model_name: Optional[str]


ProgressEmitter = Callable[[Dict[str, Any]], None]


@dataclass
class ParsedBlock:
    block_index: int
    type: str
    text: str
    heading: Optional[str] = None
    level: Optional[int] = None
    page_no: Optional[int] = None
    markdown: Optional[str] = None
    html: Optional[str] = None
    asset_path: Optional[str] = None
    alt_text: Optional[str] = None
    caption: Optional[str] = None
    ocr_text: Optional[str] = None

    def to_dict(self) -> Dict[str, Any]:
        return asdict(self)


@dataclass
class ParsedChunk:
    heading: Optional[str]
    page_no: Optional[int]
    text: str
    order: int
    score: float = 1.0
    block_indexes: Optional[List[int]] = None

    def to_dict(self) -> Dict[str, Any]:
        return asdict(self)


@dataclass
class ParsedDocument:
    title: Optional[str]
    file_type: str
    content: str
    chunks: List[ParsedChunk]
    blocks: Optional[List[ParsedBlock]] = None

    def to_dict(self) -> Dict[str, Any]:
        return {
            "title": self.title,
            "file_type": self.file_type,
            "content": self.content,
            "chunks": [chunk.to_dict() for chunk in self.chunks],
            "blocks": [block.to_dict() for block in self.blocks] if self.blocks else None,
        }


@dataclass
class ParserError:
    code: str
    message: str
    details: Optional[str] = None

    def to_dict(self) -> Dict[str, Any]:
        return asdict(self)


@dataclass
class EmbeddingStatus:
    available: bool
    provider: str
    model_name: str
    model_path: str
    dimension: int
    message: str

    def to_dict(self) -> Dict[str, Any]:
        return asdict(self)


@dataclass
class EmbeddingResponse:
    vectors: List[List[float]]
    status: EmbeddingStatus

    def to_dict(self) -> Dict[str, Any]:
        return {
            "vectors": self.vectors,
            "status": self.status.to_dict(),
        }


@dataclass
class ParserStreamMessage:
    request_id: str
    kind: str
    event: str
    message: str
    stage: str
    percent: int = 0
    current: str = ""
    total: int = 0
    processed: int = 0
    parser_source: str = ""
    warning: Optional[str] = None
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
        texts=[str(item) for item in (data.get("texts") or []) if str(item).strip()],
        model_name=(
            str(data["model_name"]).strip() if str(data.get("model_name", "")).strip() else None
        ),
    )
