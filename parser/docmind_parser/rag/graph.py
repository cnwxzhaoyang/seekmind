"""
@author MorningSun
@CreatedDate 2026/06/04
@Description Python sidecar RAG 的 graph 状态与节点实现。
"""

from __future__ import annotations

from dataclasses import dataclass, field
import json
import re
from typing import Any, Dict, List, Optional, Sequence, Tuple, TypedDict
import sys

from langgraph.graph import END, START, StateGraph

from .client import ask_model_stream, ask_model_text
from .db import RagChunkRecord, RagStore
from .models import ProgressEmitter, RagEvent, RagRequest, RagResponse, RagRetrieval, RagSource
from .prompt import build_judge_prompt, build_system_prompt
from .retrieval import (
    RagRecallCandidate,
    RagContext,
    build_session_terms,
    collect_recall_candidates,
    pack_context_from_hits,
    select_recall_hits,
)
from .verifier import build_citation_warning
from ..models import ParserError


@dataclass
class QuestionRewrite:
    retrieval_question: str
    relation_hint: Optional[str] = None


@dataclass
class RagGraphState:
    request: RagRequest
    rewrite: QuestionRewrite
    session_terms: List[str] = field(default_factory=list)
    candidates: Dict[str, RagRecallCandidate] = field(default_factory=dict)
    selected_hits: List[RagChunkRecord] = field(default_factory=list)
    context: Optional[RagContext] = None
    answer: str = ""
    warning: Optional[str] = None
    retry_count: int = 0
    repair_hint: Optional[str] = None
    judge_should_repair: bool = False
    judge_reason: str = ""
    judge_confidence: float = 0.0
    response: Optional[RagResponse] = None


class RagGraphStateData(TypedDict, total=False):
    request: RagRequest
    rewrite: QuestionRewrite
    session_terms: List[str]
    candidates: Dict[str, RagRecallCandidate]
    selected_hits: List[RagChunkRecord]
    context: Optional[RagContext]
    answer: str
    warning: Optional[str]
    retry_count: int
    repair_hint: Optional[str]
    judge_should_repair: bool
    judge_reason: str
    judge_confidence: float
    response: RagResponse


def _log(message: str) -> None:
    sys.stderr.write(f"[docmind:rag] {message}\n")
    sys.stderr.flush()


def _emit(emit: Optional[ProgressEmitter], event: RagEvent) -> None:
    if emit is None:
        return
    emit(event.to_dict())


def _state_to_obj(state: RagGraphStateData) -> RagGraphState:
    # 修复：LangGraph 节点之间用结构化 state 传递，落到已有业务函数时再转换成 dataclass，避免重复改写主逻辑。
    return RagGraphState(
        request=state["request"],
        rewrite=state["rewrite"],
        session_terms=list(state.get("session_terms", [])),
        candidates=dict(state.get("candidates", {})),
        selected_hits=list(state.get("selected_hits", [])),
        context=state.get("context"),
        answer=state.get("answer", ""),
        warning=state.get("warning"),
        retry_count=state.get("retry_count", 0),
        repair_hint=state.get("repair_hint"),
        judge_should_repair=state.get("judge_should_repair", False),
        judge_reason=state.get("judge_reason", ""),
        judge_confidence=state.get("judge_confidence", 0.0),
        response=state.get("response"),
    )


def _state_updates(state_obj: RagGraphState) -> RagGraphStateData:
    return {
        "request": state_obj.request,
        "rewrite": state_obj.rewrite,
        "session_terms": list(state_obj.session_terms),
        "candidates": dict(state_obj.candidates),
        "selected_hits": list(state_obj.selected_hits),
        "context": state_obj.context,
        "answer": state_obj.answer,
        "warning": state_obj.warning,
        "retry_count": state_obj.retry_count,
        "repair_hint": state_obj.repair_hint,
        "judge_should_repair": state_obj.judge_should_repair,
        "judge_reason": state_obj.judge_reason,
        "judge_confidence": state_obj.judge_confidence,
        "response": state_obj.response,
    }


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


def prepare_graph_state(request: RagRequest) -> RagGraphState:
    rewrite = rewrite_follow_up_question(request.question, request.recent_questions)
    session_terms = build_session_terms(request.session_context)
    return RagGraphState(
        request=request,
        rewrite=rewrite,
        session_terms=session_terms,
    )


def retrieve_candidates(
    state: RagGraphState,
    store: RagStore,
    emit: Optional[ProgressEmitter] = None,
) -> RagGraphState:
    _emit(
        emit,
        RagEvent(
            request_id=state.request.request_id,
            kind="event",
            event="progress",
            stage="retrieve",
            message="正在从 SQLite 召回证据",
            percent=20,
        ),
    )
    candidates, _, _ = collect_recall_candidates(
        store=store,
        question=state.rewrite.retrieval_question,
        scope_paths=state.request.scope_paths,
        settings=state.request.settings,
        limit=state.request.settings.context_chunk_limit,
        session_terms=state.session_terms,
    )
    state.candidates = candidates
    _log(
        f"rag request {state.request.request_id} retrieved candidates={len(state.candidates)}"
    )
    return state


def rank_candidates(
    state: RagGraphState,
    emit: Optional[ProgressEmitter] = None,
) -> RagGraphState:
    _emit(
        emit,
        RagEvent(
            request_id=state.request.request_id,
            kind="event",
            event="progress",
            stage="rank",
            message="正在筛选和重排证据",
            percent=35,
        ),
    )
    state.selected_hits = select_recall_hits(
        candidates=state.candidates,
        scope_paths=state.request.scope_paths,
        settings=state.request.settings,
        limit=state.request.settings.context_chunk_limit,
    )
    _log(
        f"rag request {state.request.request_id} ranked selected_hits={len(state.selected_hits)}"
    )
    return state


def pack_evidence(
    state: RagGraphState,
    store: RagStore,
    emit: Optional[ProgressEmitter] = None,
) -> RagGraphState:
    _emit(
        emit,
        RagEvent(
            request_id=state.request.request_id,
            kind="event",
            event="progress",
            stage="pack_evidence",
            message="正在组装证据窗口",
            percent=45,
        ),
    )
    state.context = pack_context_from_hits(
        store=store,
        selected_hits=state.selected_hits,
        settings=state.request.settings,
        limit=state.request.settings.context_chunk_limit,
    )
    state.context.retrieval.candidate_count = len(state.candidates)
    _log(
        f"rag request {state.request.request_id} packed sources={len(state.context.sources)}"
    )
    return state


def draft_answer(
    state: RagGraphState,
    emit: Optional[ProgressEmitter] = None,
) -> RagGraphState:
    if state.context is None:
        raise RuntimeError("missing rag context")

    _emit(
        emit,
        RagEvent(
            request_id=state.request.request_id,
            kind="event",
            event="progress",
            stage="prompt",
            message="正在组装 prompt",
            percent=50,
        ),
    )
    system_prompt = build_system_prompt(
        state.context,
        state.request.session_context,
        relation_hint=state.rewrite.relation_hint,
    )

    _emit(
        emit,
        RagEvent(
            request_id=state.request.request_id,
            kind="event",
            event="progress",
            stage="generate",
            message="正在调用模型生成答案",
            percent=60,
        ),
    )
    answer_parts: List[str] = []
    try:
        for delta in ask_model_stream(
            base_url=state.request.settings.base_url,
            api_key=state.request.settings.api_key,
            model=state.request.settings.model,
            question=state.rewrite.retrieval_question,
            prompt=system_prompt,
            temperature=state.request.settings.temperature,
            max_output_tokens=state.request.settings.max_output_tokens,
        ):
            answer_parts.append(delta)
            _emit(
                emit,
                RagEvent(
                    request_id=state.request.request_id,
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
            f"rag request {state.request.request_id} streaming failed, fallback to non-stream: {error}"
        )
        answer_parts = []
        answer = ask_model_text(
            base_url=state.request.settings.base_url,
            api_key=state.request.settings.api_key,
            model=state.request.settings.model,
            question=state.rewrite.retrieval_question,
            prompt=system_prompt,
            temperature=state.request.settings.temperature,
            max_output_tokens=state.request.settings.max_output_tokens,
        )
        answer_parts.append(answer)

    state.answer = "".join(answer_parts).strip()
    if not state.answer:
        state.answer = ask_model_text(
            base_url=state.request.settings.base_url,
            api_key=state.request.settings.api_key,
            model=state.request.settings.model,
            question=state.rewrite.retrieval_question,
            prompt=system_prompt,
            temperature=state.request.settings.temperature,
            max_output_tokens=state.request.settings.max_output_tokens,
        ).strip()
    return state


def verify_answer(
    state: RagGraphState,
    emit: Optional[ProgressEmitter] = None,
) -> RagGraphState:
    source_ids = [source.source.source_id for source in state.context.sources] if state.context else []
    state.warning = build_citation_warning(state.answer, source_ids)
    _emit(
        emit,
        RagEvent(
            request_id=state.request.request_id,
            kind="event",
            event="progress",
            stage="verify",
            message="正在校验引用",
            answer_delta="",
            percent=85,
            warning=state.warning,
        ),
    )
    if state.warning:
        _log(f"rag request {state.request.request_id} citation warning: {state.warning}")
    return state


def _parse_judge_payload(raw_text: str) -> Tuple[bool, str, float]:
    cleaned = (raw_text or "").strip()
    if not cleaned:
        return True, "模型未返回有效判定结果", 0.0
    try:
        payload = json.loads(cleaned)
    except Exception:  # noqa: BLE001
        match = re.search(r"\{.*\}", cleaned, re.S)
        if not match:
            return True, "模型判定结果无法解析", 0.0
        try:
            payload = json.loads(match.group(0))
        except Exception:  # noqa: BLE001
            return True, "模型判定结果无法解析", 0.0

    should_repair = bool(payload.get("should_repair", True))
    reason = str(payload.get("reason") or "").strip() or "模型判定需要修复"
    try:
        confidence = float(payload.get("confidence", 0.0))
    except Exception:  # noqa: BLE001
        confidence = 0.0
    confidence = max(0.0, min(confidence, 1.0))
    return should_repair, reason, confidence


def judge_answer(
    state: RagGraphState,
    emit: Optional[ProgressEmitter] = None,
) -> RagGraphState:
    if state.context is None:
        return state

    _emit(
        emit,
        RagEvent(
            request_id=state.request.request_id,
            kind="event",
            event="progress",
            stage="judge",
            message="正在判断答案是否需要修复",
            percent=88,
            warning=state.warning,
        ),
    )
    judge_prompt = build_judge_prompt(
        state.context,
        state.answer,
        warning=state.warning,
    )
    try:
        judge_raw = ask_model_text(
            base_url=state.request.settings.base_url,
            api_key=state.request.settings.api_key,
            model=state.request.settings.model,
            question=state.rewrite.retrieval_question,
            prompt=judge_prompt,
            temperature=0.0,
            max_output_tokens=256,
        )
        should_repair, reason, confidence = _parse_judge_payload(judge_raw)
    except Exception as error:  # noqa: BLE001
        # 修复：judge 是质量判定节点，失败时不能吞掉已生成答案；保留规则 warning，由前端提示用户。
        _log(f"rag request {state.request.request_id} judge failed: {error}")
        should_repair, reason, confidence = False, f"judge_failed: {error}", 0.0

    if state.warning and not should_repair:
        # 修复：规则层已经发现引用缺失时，LLM judge 不能直接推翻规则警告，只能决定是否继续修复或保守放行。
        _log(
            f"rag request {state.request.request_id} judge overruled warning with reason={reason}"
        )

    state.judge_should_repair = should_repair
    state.judge_reason = reason
    state.judge_confidence = confidence
    if should_repair:
        state.repair_hint = (
            f"judge 认为答案需要修复：{reason}。"
            "请只保留被来源直接支撑的句子，删掉未被来源支撑的内容。"
        )
    _log(
        f"rag request {state.request.request_id} judge result "
        f"should_repair={should_repair} confidence={confidence:.2f} reason={reason}"
    )
    return state


def repair_answer(
    state: RagGraphState,
    emit: Optional[ProgressEmitter] = None,
) -> RagGraphState:
    if not state.judge_should_repair or state.retry_count >= 1 or state.context is None:
        return state

    _emit(
        emit,
        RagEvent(
            request_id=state.request.request_id,
            kind="event",
            event="progress",
            stage="repair",
            message="正在修复引用不足的答案",
            percent=90,
            warning=state.warning,
        ),
    )

    repair_hint = state.repair_hint or (
        f"上一版答案未通过引用校验：{state.warning}。"
        "请基于现有来源重新组织回答，只保留能被来源支撑的事实句，删除无有效引用的句子。"
        "不要复用上一版答案中的无引用句子。"
    )
    # 修复：修复阶段不再走流式增量，避免当前 UI 把第二轮 delta 继续拼接到第一轮正文后面。
    repair_prompt = build_system_prompt(
        state.context,
        state.request.session_context,
        relation_hint=state.rewrite.relation_hint,
        repair_hint=repair_hint,
    )
    try:
        repaired_answer = ask_model_text(
            base_url=state.request.settings.base_url,
            api_key=state.request.settings.api_key,
            model=state.request.settings.model,
            question=state.rewrite.retrieval_question,
            prompt=repair_prompt,
            temperature=state.request.settings.temperature,
            max_output_tokens=state.request.settings.max_output_tokens,
        ).strip()
    except Exception as error:  # noqa: BLE001
        # 修复：repair 是增强节点，二次生成失败时保留第一轮答案，避免可用回答被降级成空失败。
        _log(f"rag request {state.request.request_id} repair failed: {error}")
        state.judge_should_repair = False
        state.retry_count += 1
        return state
    if repaired_answer:
        state.answer = repaired_answer
    state.retry_count += 1
    state.judge_should_repair = False
    return state


def finalize(
    state: RagGraphState,
    emit: Optional[ProgressEmitter] = None,
) -> RagResponse:
    if state.context is None:
        return RagResponse(
            kind="response",
            request_id=state.request.request_id,
            ok=False,
            answer="",
            state="failed",
            warning=state.warning,
            error=ParserError(code="rag_pipeline_failed", message="missing rag context"),
            retrieval=None,
            sources=[],
        )

    has_answer = bool(state.answer.strip())
    response = RagResponse(
        kind="response",
        request_id=state.request.request_id,
        ok=has_answer,
        answer=state.answer,
        state="answered" if has_answer else "failed",
        warning=state.warning,
        error=None if has_answer else ParserError(code="rag_pipeline_failed", message="empty answer"),
        retrieval=state.context.retrieval,
        sources=[source_block.source for source_block in state.context.sources],
    )
    _emit(
        emit,
        RagEvent(
            request_id=state.request.request_id,
            kind="event",
            event="progress",
            stage="finalize",
            message="RAG 编排完成",
            answer_delta="",
            percent=100,
            warning=state.warning,
        ),
    )
    _log(
        f"rag request {state.request.request_id} completed "
        f"sources={len(response.sources)} warning={'yes' if state.warning else 'no'}"
    )
    return response


def _node_retrieve(state: RagGraphStateData, store: RagStore, emit: Optional[ProgressEmitter]) -> RagGraphStateData:
    obj = _state_to_obj(state)
    retrieve_candidates(obj, store, emit)
    return _state_updates(obj)


def _node_rank(state: RagGraphStateData, emit: Optional[ProgressEmitter]) -> RagGraphStateData:
    obj = _state_to_obj(state)
    rank_candidates(obj, emit)
    return _state_updates(obj)


def _node_pack(state: RagGraphStateData, store: RagStore, emit: Optional[ProgressEmitter]) -> RagGraphStateData:
    obj = _state_to_obj(state)
    pack_evidence(obj, store, emit)
    return _state_updates(obj)


def _node_draft(state: RagGraphStateData, emit: Optional[ProgressEmitter]) -> RagGraphStateData:
    obj = _state_to_obj(state)
    draft_answer(obj, emit)
    return _state_updates(obj)


def _node_verify(state: RagGraphStateData, emit: Optional[ProgressEmitter]) -> RagGraphStateData:
    obj = _state_to_obj(state)
    verify_answer(obj, emit)
    return _state_updates(obj)


def _node_judge(state: RagGraphStateData, emit: Optional[ProgressEmitter]) -> RagGraphStateData:
    obj = _state_to_obj(state)
    judge_answer(obj, emit)
    return _state_updates(obj)


def _node_repair(state: RagGraphStateData, emit: Optional[ProgressEmitter]) -> RagGraphStateData:
    obj = _state_to_obj(state)
    repair_answer(obj, emit)
    return _state_updates(obj)


def _node_finalize(state: RagGraphStateData, emit: Optional[ProgressEmitter]) -> RagGraphStateData:
    obj = _state_to_obj(state)
    obj.response = finalize(obj, emit)
    return _state_updates(obj)


def _route_after_pack(state: RagGraphStateData) -> str:
    context = state.get("context")
    if context is None or not context.sources:
        return "finalize"
    return "draft"


def _route_after_verify(state: RagGraphStateData) -> str:
    return "judge"


def _route_after_judge(state: RagGraphStateData) -> str:
    if state.get("judge_should_repair") and state.get("retry_count", 0) < 1:
        return "repair"
    return "finalize"


def build_rag_langgraph(store: RagStore, emit: Optional[ProgressEmitter] = None):
    graph = StateGraph(RagGraphStateData)

    graph.add_node("retrieve", lambda state: _node_retrieve(state, store, emit))
    graph.add_node("rank", lambda state: _node_rank(state, emit))
    graph.add_node("pack", lambda state: _node_pack(state, store, emit))
    graph.add_node("draft", lambda state: _node_draft(state, emit))
    graph.add_node("verify", lambda state: _node_verify(state, emit))
    graph.add_node("repair", lambda state: _node_repair(state, emit))
    graph.add_node("finalize", lambda state: _node_finalize(state, emit))

    graph.add_edge(START, "retrieve")
    graph.add_edge("retrieve", "rank")
    graph.add_edge("rank", "pack")
    graph.add_conditional_edges(
        "pack",
        _route_after_pack,
        {
            "draft": "draft",
            "finalize": "finalize",
        },
    )
    graph.add_edge("draft", "verify")
    graph.add_conditional_edges(
        "verify",
        _route_after_verify,
        {
            "judge": "judge",
        },
    )
    graph.add_node("judge", lambda state: _node_judge(state, emit))
    graph.add_conditional_edges(
        "judge",
        _route_after_judge,
        {
            "repair": "repair",
            "finalize": "finalize",
        },
    )
    graph.add_edge("repair", "verify")
    graph.add_edge("finalize", END)
    return graph.compile()
