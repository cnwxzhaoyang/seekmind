"""
@author MorningSun
@CreatedDate 2026/06/03
@Description Python sidecar RAG 的流式编排入口。
"""

from __future__ import annotations

from dataclasses import dataclass
import sys
from typing import Optional, Sequence, Tuple

from ..models import ParserError
from .client import ask_model, ask_model_stream
from .db import RagStore
from .models import RagEvent, RagRequest, RagResponse, RagRetrieval, ProgressEmitter
from .prompt import build_system_prompt
from .retrieval import RagContext, build_qa_context, build_session_terms
from .verifier import build_citation_warning


@dataclass
class QuestionRewrite:
    retrieval_question: str
    relation_hint: Optional[str] = None


def _is_relation_follow_up(question: str) -> bool:
    return any(marker in question for marker in ("这两者", "这二者", "二者", "两者", "它们", "前面两个", "前两个"))


def _normalize_reference_subject(question: str) -> str:
    subject = question.strip()
    for prefix in ("什么是", "何为", "请问", "介绍一下", "解释一下", "请介绍", "请解释"):
        subject = subject.removeprefix(prefix).strip()
    for suffix in ("是什么", "是啥", "指什么"):
        subject = subject.removesuffix(suffix).strip()
    return subject.strip("？?。 \t\r\n")


def _infer_relation_subjects_from_questions(questions: Sequence[str]) -> Optional[Tuple[str, str]]:
    subjects = []
    for question in reversed(list(questions)):
        subject = _normalize_reference_subject(question)
        if not subject or _is_relation_follow_up(subject):
            continue
        subjects.append(subject)
        if len(subjects) == 2:
            return subjects[1], subjects[0]
    return None


def rewrite_follow_up_question(question: str, recent_questions: Sequence[str]) -> QuestionRewrite:
    if not _is_relation_follow_up(question):
        return QuestionRewrite(retrieval_question=question)

    relation_subjects = _infer_relation_subjects_from_questions(recent_questions)
    if relation_subjects is None:
        return QuestionRewrite(retrieval_question=question)

    left, right = relation_subjects
    rewritten = f"{left} 与 {right} 的关系"
    hint = f"当前问题中的“这两者”指代：{left} 与 {right}。请围绕这两个对象回答。"
    _log(
        "question rewrite applied "
        f"question={question[:80]} rewritten={rewritten[:80]}"
    )
    return QuestionRewrite(retrieval_question=rewritten, relation_hint=hint)


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
            _emit(
                emit,
                RagEvent(
                    request_id=request.request_id,
                    kind="event",
                    event="progress",
                    stage="retrieve",
                    message="正在从 SQLite 召回证据",
                    percent=20,
                ),
            )
            rewrite = rewrite_follow_up_question(request.question, request.recent_questions)
            session_terms = build_session_terms(request.session_context)
            context: RagContext = build_qa_context(
                store=store,
                question=rewrite.retrieval_question,
                scope_paths=request.scope_paths,
                settings=request.settings,
                limit=request.settings.context_chunk_limit,
                session_terms=session_terms,
                emit_progress=emit,
            )

            if not context.sources:
                warning = "当前范围内没有可用证据"
                _log(f"rag request {request.request_id} has no sources")
                _emit(
                    emit,
                    RagEvent(
                        request_id=request.request_id,
                        kind="event",
                        event="progress",
                        stage="retrieve",
                        message=warning,
                        percent=100,
                        warning=warning,
                    ),
                )
                return RagResponse(
                    kind="response",
                    request_id=request.request_id,
                    ok=False,
                    answer="",
                    state="insufficient",
                    warning=warning,
                    error=ParserError(code="insufficient_evidence", message=warning),
                    retrieval=context.retrieval,
                    sources=[],
                )

            _emit(
                emit,
                RagEvent(
                    request_id=request.request_id,
                    kind="event",
                    event="progress",
                    stage="prompt",
                    message="正在组装 prompt",
                    percent=45,
                ),
            )
            system_prompt = build_system_prompt(
                context,
                request.session_context,
                relation_hint=rewrite.relation_hint,
            )

            _emit(
                emit,
                RagEvent(
                    request_id=request.request_id,
                    kind="event",
                    event="progress",
                    stage="generate",
                    message="正在调用模型生成答案",
                    percent=60,
                ),
            )
            answer_parts = []
            try:
                for delta in ask_model_stream(
                    base_url=request.settings.base_url,
                    api_key=request.settings.api_key,
                    model=request.settings.model,
                    question=rewrite.retrieval_question,
                    prompt=system_prompt,
                    temperature=request.settings.temperature,
                    max_output_tokens=request.settings.max_output_tokens,
                ):
                    answer_parts.append(delta)
                    _emit(
                        emit,
                        RagEvent(
                            request_id=request.request_id,
                            kind="event",
                            event="progress",
                            stage="answer_delta",
                            message="正在生成答案",
                            answer_delta=delta,
                            percent=72,
                        ),
                    )
            except Exception as error:  # noqa: BLE001
                _log(
                    f"rag request {request.request_id} streaming failed, fallback to non-stream: {error}"
                )
                answer_parts = []
                answer = ask_model(
                    base_url=request.settings.base_url,
                    api_key=request.settings.api_key,
                    model=request.settings.model,
                    question=rewrite.retrieval_question,
                    prompt=system_prompt,
                    temperature=request.settings.temperature,
                    max_output_tokens=request.settings.max_output_tokens,
                )
                answer_parts.append(answer)

            answer = "".join(answer_parts).strip()
            if not answer:
                answer = ask_model(
                    base_url=request.settings.base_url,
                    api_key=request.settings.api_key,
                    model=request.settings.model,
                    question=rewrite.retrieval_question,
                    prompt=system_prompt,
                    temperature=request.settings.temperature,
                    max_output_tokens=request.settings.max_output_tokens,
                )

            source_ids = [source.source.source_id for source in context.sources]
            warning = build_citation_warning(answer, source_ids)

            _emit(
                emit,
                RagEvent(
                    request_id=request.request_id,
                    kind="event",
                    event="progress",
                    stage="verify",
                    message="正在校验引用",
                    answer_delta="",
                    percent=85,
                    warning=warning,
                ),
            )

            if warning:
                _log(f"rag request {request.request_id} citation warning: {warning}")

            response = RagResponse(
                kind="response",
                request_id=request.request_id,
                ok=True,
                answer=answer,
                state="answered",
                warning=warning,
                error=None,
                retrieval=context.retrieval,
                sources=[source_block.source for source_block in context.sources],
            )

            _emit(
                emit,
                RagEvent(
                    request_id=request.request_id,
                    kind="event",
                    event="progress",
                    stage="finish",
                    message="RAG 编排完成",
                    answer_delta="",
                    percent=100,
                    warning=warning,
                ),
            )
            _log(
                f"rag request {request.request_id} completed "
                f"sources={len(response.sources)} warning={'yes' if warning else 'no'}"
            )
            return response
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
