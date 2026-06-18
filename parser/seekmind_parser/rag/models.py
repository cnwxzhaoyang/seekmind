"""
@author MorningSun
@CreatedDate 2026/06/03
@Description Python sidecar RAG 请求、事件、响应与来源模型。
"""

from __future__ import annotations

from dataclasses import asdict, dataclass, field
from typing import Any, Callable, Dict, List, Optional

from ..models import ParserError


def _clean_text(value: Any) -> str:
    return str(value).strip() if value is not None else ""


def _clean_int(value: Any, default: int) -> int:
    try:
        if value is None or value == "":
            return default
        return int(value)
    except Exception:  # noqa: BLE001
        return default


def _clean_float(value: Any, default: float) -> float:
    try:
        if value is None or value == "":
            return default
        return float(value)
    except Exception:  # noqa: BLE001
        return default


def _clean_bool(value: Any, default: bool = False) -> bool:
    if value is None:
        return default
    if isinstance(value, bool):
        return value
    if isinstance(value, (int, float)):
        return bool(value)
    lowered = str(value).strip().lower()
    if lowered in {"1", "true", "yes", "on"}:
        return True
    if lowered in {"0", "false", "no", "off"}:
        return False
    return default


def _clean_list(values: Any) -> List[str]:
    if not isinstance(values, list):
        return []
    cleaned = []
    for value in values:
        text = _clean_text(value)
        if text:
            cleaned.append(text)
    return cleaned


@dataclass
class RagSettings:
    provider: str = ""
    base_url: str = ""
    api_key: str = ""
    model: str = ""
    temperature: float = 0.2
    max_output_tokens: int = 6000
    context_chunk_limit: int = 8
    context_token_budget: int = 6000
    min_evidence_count: int = 1
    min_retrieval_score: float = 0.0
    intent_synonym_rules_json: str = ""

    @classmethod
    def from_dict(cls, data: Optional[Dict[str, Any]]) -> "RagSettings":
        payload = data or {}
        return cls(
            provider=_clean_text(payload.get("provider")),
            base_url=_clean_text(payload.get("base_url")),
            api_key=_clean_text(payload.get("api_key")),
            model=_clean_text(payload.get("model")),
            temperature=_clean_float(payload.get("temperature"), 0.2),
            max_output_tokens=_clean_int(payload.get("max_output_tokens"), 6000),
            context_chunk_limit=_clean_int(payload.get("context_chunk_limit"), 8),
            context_token_budget=_clean_int(payload.get("context_token_budget"), 6000),
            min_evidence_count=_clean_int(payload.get("min_evidence_count"), 1),
            min_retrieval_score=_clean_float(payload.get("min_retrieval_score"), 0.0),
            intent_synonym_rules_json=_clean_text(payload.get("intent_synonym_rules_json")),
        )

    def to_dict(self) -> Dict[str, Any]:
        return asdict(self)


@dataclass
class RagRequest:
    request_id: str
    command: str
    db_path: str
    question: str
    session_id: Optional[str] = None
    scope_paths: List[str] = field(default_factory=list)
    session_context: str = ""
    recent_questions: List[str] = field(default_factory=list)
    settings: RagSettings = field(default_factory=RagSettings)

    def to_dict(self) -> Dict[str, Any]:
        return {
            "request_id": self.request_id,
            "command": self.command,
            "db_path": self.db_path,
            "question": self.question,
            "session_id": self.session_id,
            "scope_paths": self.scope_paths,
            "session_context": self.session_context,
            "recent_questions": self.recent_questions,
            "settings": self.settings.to_dict(),
        }


@dataclass
class RagEvent:
    request_id: str
    kind: str
    event: str
    stage: str
    message: str
    answer_delta: str = ""
    percent: int = 0
    current: str = ""
    total: int = 0
    processed: int = 0
    warning: Optional[str] = None
    details: Optional[str] = None

    def to_dict(self) -> Dict[str, Any]:
        return asdict(self)


@dataclass
class RagSource:
    source_id: str
    chunk_id: str
    file_name: str
    path: str
    ext: str
    title_path: str
    heading: str
    paragraph: Optional[int]
    page: Optional[int]
    snippet: str
    score: float
    rank_reason: str
    preview_blocks: List[Dict[str, Any]] = field(default_factory=list)

    def to_dict(self) -> Dict[str, Any]:
        return asdict(self)


@dataclass
class RagRetrieval:
    search_mode: str = "python_rag_skeleton"
    candidate_count: int = 0
    selected_count: int = 0
    semantic_enabled: bool = False
    semantic_fallback: bool = False
    semantic_fallback_reason: str = ""

    def to_dict(self) -> Dict[str, Any]:
        return asdict(self)


@dataclass
class RagResponse:
    kind: str
    request_id: str
    ok: bool
    answer: str = ""
    state: str = "failed"
    warning: Optional[str] = None
    error: Optional[ParserError] = None
    retrieval: Optional[RagRetrieval] = None
    sources: List[RagSource] = field(default_factory=list)

    def to_dict(self) -> Dict[str, Any]:
        return {
            "kind": self.kind,
            "request_id": self.request_id,
            "ok": self.ok,
            "answer": self.answer,
            "state": self.state,
            "warning": self.warning,
            "error": self.error.to_dict() if self.error else None,
            "retrieval": self.retrieval.to_dict() if self.retrieval else None,
            "sources": [source.to_dict() for source in self.sources],
        }


ProgressEmitter = Callable[[Dict[str, Any]], None]


def rag_request_from_dict(data: Dict[str, Any]) -> RagRequest:
    options = data.get("options") or {}
    settings = RagSettings.from_dict(options.get("settings") or data.get("settings"))
    scope_paths = _clean_list(options.get("scope_paths") or data.get("scope_paths"))
    question = _clean_text(options.get("question") or data.get("question"))
    db_path = _clean_text(options.get("db_path") or data.get("db_path"))
    session_context = _clean_text(options.get("session_context") or data.get("session_context"))
    session_id = _clean_text(options.get("session_id") or data.get("session_id")) or None
    recent_questions = _clean_list(options.get("recent_questions") or data.get("recent_questions"))

    return RagRequest(
        request_id=_clean_text(data.get("request_id")),
        command=_clean_text(data.get("command")),
        db_path=db_path,
        question=question,
        session_id=session_id,
        scope_paths=scope_paths,
        session_context=session_context,
        recent_questions=recent_questions,
        settings=settings,
    )
