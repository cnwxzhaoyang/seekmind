"""
@author MorningSun
@CreatedDate 2026/06/05
@Description Python sidecar RAG 回归评测入口与样本执行器。
"""

from __future__ import annotations

from dataclasses import asdict, dataclass, field
from typing import Any, Dict, List, Optional, Sequence

from .db import RagStore
from .graph import build_rag_langgraph, prepare_graph_state
from .models import RagRequest, RagResponse, RagSettings, rag_request_from_dict


@dataclass
class RagEvalCase:
    id: str
    question: str
    expected_state: str = "answered"
    scope_paths: List[str] = field(default_factory=list)
    recent_questions: List[str] = field(default_factory=list)
    session_context: str = ""
    min_answer_chars: int = 1
    warning_contains: Optional[str] = None

    def to_dict(self) -> Dict[str, Any]:
        return asdict(self)


@dataclass
class RagEvalCaseResult:
    id: str
    question: str
    expected_state: str
    actual_state: str
    passed: bool
    answer_chars: int
    warning: Optional[str]
    error: Optional[str]
    answer_excerpt: str = ""

    def to_dict(self) -> Dict[str, Any]:
        return asdict(self)


@dataclass
class RagEvalReport:
    kind: str
    request_id: str
    ok: bool
    total: int
    passed: int
    failed: int
    results: List[RagEvalCaseResult]
    warning: Optional[str] = None

    def to_dict(self) -> Dict[str, Any]:
        return {
            "kind": self.kind,
            "request_id": self.request_id,
            "ok": self.ok,
            "total": self.total,
            "passed": self.passed,
            "failed": self.failed,
            "results": [result.to_dict() for result in self.results],
            "warning": self.warning,
        }


DEFAULT_EVAL_CASES: List[RagEvalCase] = [
    RagEvalCase(id="RAG-001", question="什么是 WBS", expected_state="answered"),
    RagEvalCase(id="RAG-002", question="在 Oracle 中创建表空间", expected_state="answered"),
    RagEvalCase(id="RAG-003", question="什么是工作流引擎", expected_state="answered"),
    RagEvalCase(
        id="RAG-004",
        question="这两者有什么区别",
        expected_state="answered",
        recent_questions=["什么是 WBS", "什么是工作流引擎"],
    ),
    RagEvalCase(
        id="RAG-005",
        question="随便问一个当前库里没有证据的问题",
        expected_state="insufficient_evidence",
    ),
    RagEvalCase(
        id="RAG-006",
        question="只给一个很窄的目录范围后问跨目录问题",
        expected_state="insufficient_evidence",
        scope_paths=["/Users/zhaoyang/Documents/does-not-exist"],
    ),
]


def _normalize_text(value: str) -> str:
    return (value or "").strip()


def _default_case_settings(request: RagRequest) -> Dict[str, Any]:
    return {
        "provider": request.settings.provider,
        "base_url": request.settings.base_url,
        "api_key": request.settings.api_key,
        "model": request.settings.model,
        "temperature": request.settings.temperature,
        "max_output_tokens": request.settings.max_output_tokens,
        "context_chunk_limit": request.settings.context_chunk_limit,
        "context_token_budget": request.settings.context_token_budget,
        "min_evidence_count": request.settings.min_evidence_count,
        "min_retrieval_score": request.settings.min_retrieval_score,
    }


def _build_request_for_case(base_request: RagRequest, case: RagEvalCase, request_id: str) -> RagRequest:
    return RagRequest(
        request_id=request_id,
        command="rag_answer_stream",
        db_path=base_request.db_path,
        question=case.question,
        session_id=base_request.session_id,
        scope_paths=case.scope_paths or list(base_request.scope_paths),
        session_context=case.session_context or base_request.session_context,
        recent_questions=case.recent_questions or list(base_request.recent_questions),
        settings=RagSettings.from_dict(_default_case_settings(base_request)),
    )


def _evaluate_single_case(
    store: RagStore,
    base_request: RagRequest,
    case: RagEvalCase,
    emit=None,
) -> RagEvalCaseResult:
    request_id = f"{base_request.request_id}:{case.id}"
    request = _build_request_for_case(base_request, case, request_id)
    graph = build_rag_langgraph(store, emit)
    state = prepare_graph_state(request)
    result = graph.invoke(
        {
            "request": state.request,
            "rewrite": state.rewrite,
            "session_terms": state.session_terms,
            "candidates": state.candidates,
            "selected_hits": state.selected_hits,
            "context": state.context,
            "answer": state.answer,
            "warning": state.warning,
            "retry_count": state.retry_count,
            "repair_hint": state.repair_hint,
            "final_state": state.final_state,
            "response": state.response,
        }
    )

    response = result.get("response")
    actual_state = response.state if isinstance(response, RagResponse) else "failed"
    answer = response.answer if isinstance(response, RagResponse) else ""
    warning = response.warning if isinstance(response, RagResponse) else None
    error = None
    if isinstance(response, RagResponse) and response.error:
        error = response.error.message or response.error.code

    answer_text = _normalize_text(answer)
    passed = actual_state == case.expected_state
    if case.expected_state == "answered":
        passed = passed and len(answer_text) >= max(case.min_answer_chars, 1)
    if case.expected_state == "insufficient_evidence":
        passed = passed and actual_state == "insufficient_evidence"
    if case.warning_contains:
        passed = passed and bool(warning and case.warning_contains in warning)

    return RagEvalCaseResult(
        id=case.id,
        question=case.question,
        expected_state=case.expected_state,
        actual_state=actual_state,
        passed=passed,
        answer_chars=len(answer_text),
        warning=warning,
        error=error,
        answer_excerpt=answer_text[:120],
    )


def run_rag_regression(
    request: RagRequest,
    cases: Optional[Sequence[RagEvalCase]] = None,
    emit=None,
) -> RagEvalReport:
    sample_cases = list(cases) if cases else list(DEFAULT_EVAL_CASES)
    results: List[RagEvalCaseResult] = []
    with RagStore.open(request.db_path) as store:
        for case in sample_cases:
            results.append(_evaluate_single_case(store, request, case, emit=emit))

    passed = sum(1 for result in results if result.passed)
    total = len(results)
    failed = total - passed
    warning = None if failed == 0 else f"{failed} / {total} 个 RAG 样本未通过回归"
    return RagEvalReport(
        kind="response",
        request_id=request.request_id,
        ok=failed == 0,
        total=total,
        passed=passed,
        failed=failed,
        results=results,
        warning=warning,
    )


def rag_eval_request_from_dict(data: Dict[str, Any]) -> tuple[RagRequest, List[RagEvalCase]]:
    base_request = rag_request_from_dict(data)
    cases_payload = data.get("cases") or []
    cases: List[RagEvalCase] = []
    for item in cases_payload:
        if not isinstance(item, dict):
            continue
        cases.append(
            RagEvalCase(
                id=str(item.get("id", "")).strip() or f"CASE-{len(cases) + 1:03d}",
                question=str(item.get("question", "")).strip(),
                expected_state=str(item.get("expected_state", "answered")).strip() or "answered",
                scope_paths=[str(path).strip() for path in (item.get("scope_paths") or []) if str(path).strip()],
                recent_questions=[
                    str(question).strip()
                    for question in (item.get("recent_questions") or [])
                    if str(question).strip()
                ],
                session_context=str(item.get("session_context", "")).strip(),
                min_answer_chars=int(item.get("min_answer_chars", 1) or 1),
                warning_contains=(
                    str(item.get("warning_contains", "")).strip()
                    or None
                ),
            )
        )
    return base_request, cases
