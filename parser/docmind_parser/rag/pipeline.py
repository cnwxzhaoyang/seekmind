"""
@author MorningSun
@CreatedDate 2026/06/03
@Description Python sidecar RAG 的流式编排入口。
"""

from __future__ import annotations

import sys
from typing import Optional

from ..models import ParserError
from .db import RagStore
from .graph import build_rag_langgraph, prepare_graph_state
from .models import RagEvent, RagRequest, RagResponse, RagRetrieval, ProgressEmitter


def _log(message: str) -> None:
    sys.stderr.write(f"[docmind:rag] {message}\n")
    sys.stderr.flush()


def _emit(emit: Optional[ProgressEmitter], event: RagEvent) -> None:
    if emit is None:
        return
    emit(event.to_dict())


def _fail(
    request_id: str,
    code: str,
    message: str,
    warning: Optional[str] = None,
    retrieval: Optional[RagRetrieval] = None,
) -> RagResponse:
    return RagResponse(
        kind="response",
        request_id=request_id,
        ok=False,
        answer="",
        state="failed",
        warning=warning,
        error=ParserError(code=code, message=message),
        retrieval=retrieval,
        sources=[],
    )


def run_rag_answer_stream(
    request: RagRequest,
    emit: Optional[ProgressEmitter] = None,
) -> RagResponse:
    if not request.request_id:
        _log("rejecting rag request: missing request_id")
        return _fail("", "invalid_request", "missing request_id")

    if not request.question.strip():
        _log(f"rejecting rag request {request.request_id}: empty question")
        _emit(
            emit,
            RagEvent(
                request_id=request.request_id,
                kind="event",
                event="progress",
                stage="validate",
                message="问题为空，无法启动 RAG 编排",
                percent=0,
                warning="rag request question is empty",
            ),
        )
        return _fail(request.request_id, "invalid_request", "empty question")

    if not request.db_path.strip():
        _log(f"rejecting rag request {request.request_id}: missing db_path")
        return _fail(request.request_id, "invalid_request", "missing db_path")

    _log(
        "starting rag pipeline "
        f"request_id={request.request_id} "
        f"session_id={request.session_id or ''} "
        f"scope_paths={len(request.scope_paths)} "
        f"question={request.question[:120]}"
    )

    _emit(
        emit,
        RagEvent(
            request_id=request.request_id,
            kind="event",
            event="progress",
            stage="bootstrap",
            message="开始 RAG 编排",
            percent=5,
        ),
    )

    try:
        with RagStore.open(request.db_path) as store:
            graph = build_rag_langgraph(store, emit)
            initial_state = prepare_graph_state(request)
            # 修复：LangGraph 负责节点编排，Python 侧只需注入初始 state 并消费最终 response。
            state = graph.invoke(
                {
                    "request": initial_state.request,
                    "rewrite": initial_state.rewrite,
                    "session_terms": initial_state.session_terms,
                    "candidates": initial_state.candidates,
                    "selected_hits": initial_state.selected_hits,
                    "context": initial_state.context,
                    "answer": initial_state.answer,
                    "warning": initial_state.warning,
                    "retry_count": initial_state.retry_count,
                    "repair_hint": initial_state.repair_hint,
                    "response": initial_state.response,
                }
            )
            response = state.get("response")
            if isinstance(response, RagResponse):
                return response
            return _fail(
                request.request_id,
                "rag_pipeline_failed",
                "missing final response",
            )
    except Exception as error:  # noqa: BLE001
        _log(f"rag request {request.request_id} failed: {error}")
        _emit(
            emit,
            RagEvent(
                request_id=request.request_id,
                kind="event",
                event="progress",
                stage="failed",
                message="RAG 编排失败",
                answer_delta="",
                percent=100,
                warning=str(error),
                details=str(error),
            ),
        )
        return _fail(
            request.request_id,
            "rag_pipeline_failed",
            str(error),
            warning=str(error),
        )
