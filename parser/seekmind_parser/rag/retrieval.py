"""
@author MorningSun
@CreatedDate 2026/06/03
@Description Python sidecar RAG 的召回、重排与上下文组装。
"""

from __future__ import annotations

from dataclasses import dataclass
import sys
from typing import Dict, Iterable, List, Optional, Sequence, Set, Tuple

from .db import RagChunkRecord, RagStore
from .models import RagRetrieval, RagSource


@dataclass
class RagRecallCandidate:
    hit: RagChunkRecord
    query_hits: int
    best_score: float
    term_overlap: int


@dataclass
class RagSourceBlock:
    source: RagSource
    block: str


@dataclass
class RagContext:
    sources: List[RagSourceBlock]
    retrieval: RagRetrieval


def normalize_query(query: str) -> List[str]:
    return tokenize_search_text(query)


def rewrite_query_terms(query: str) -> List[str]:
    terms = tokenize_search_text(query)
    lower = query.lower()
    expanded: List[str] = []

    if any(marker in lower for marker in ("离线仓库", "offline", "repo", "repository")):
        expanded.extend(["offline", "repo", "repository", "local repository"])
    if any(marker in lower for marker in ("语义搜索", "semantic", "embedding", "向量")):
        expanded.extend(["semantic search", "embedding", "vector"])
    if any(marker in lower for marker in ("切片", "chunk", "chunks")):
        expanded.extend(["chunk", "paragraph", "snippet"])
    if any(marker in lower for marker in ("markdown", " md ", "md")):
        expanded.extend(["markdown", "md"])
    if any(marker in lower for marker in ("docx", "word")):
        expanded.extend(["docx", "word"])
    if "html" in lower:
        expanded.append("html")

    terms.extend(expanded)
    return dedupe_terms(terms)


def tokenize_search_text(input_text: str) -> List[str]:
    tokens: List[str] = []
    ascii_buffer: List[str] = []
    chinese_buffer: List[str] = []

    def flush_ascii() -> None:
        if ascii_buffer:
            tokens.append("".join(ascii_buffer).lower())
            ascii_buffer.clear()

    def flush_chinese() -> None:
        if not chinese_buffer:
            return
        if len(chinese_buffer) == 1:
            tokens.append("".join(chinese_buffer))
        else:
            for index in range(len(chinese_buffer) - 1):
                tokens.append("".join(chinese_buffer[index : index + 2]))
        chinese_buffer.clear()

    for ch in input_text:
        if ch.isascii() and ch.isalnum():
            flush_chinese()
            ascii_buffer.append(ch)
            continue

        if is_han_character(ch):
            flush_ascii()
            chinese_buffer.append(ch)
            continue

        flush_ascii()
        flush_chinese()

    flush_ascii()
    flush_chinese()
    return dedupe_terms(tokens)


def dedupe_terms(terms: Sequence[str]) -> List[str]:
    deduped: List[str] = []
    for term in terms:
        cleaned = term.strip()
        if cleaned and cleaned not in deduped:
            deduped.append(cleaned)
    return deduped


def is_han_character(ch: str) -> bool:
    code_point = ord(ch)
    return (
        0x4E00 <= code_point <= 0x9FFF
        or 0x3400 <= code_point <= 0x4DBF
        or 0x20000 <= code_point <= 0x2A6DF
        or 0x2A700 <= code_point <= 0x2B73F
        or 0x2B740 <= code_point <= 0x2B81F
        or 0x2B820 <= code_point <= 0x2CEAF
        or 0xF900 <= code_point <= 0xFAFF
    )


def _clean_scope_paths(scope_paths: Sequence[str]) -> List[str]:
    return [path.strip() for path in scope_paths if path and path.strip()]


def matches_scope(path: str, scope_paths: Sequence[str]) -> bool:
    cleaned = _clean_scope_paths(scope_paths)
    if not cleaned:
        return True
    return any(path.startswith(scope_path) for scope_path in cleaned)


def build_location_label(source: RagChunkRecord) -> str:
    if source.heading.strip():
        return source.heading
    return source.file_name


def build_prompt_block(
    source_id: str,
    file_name: str,
    path: str,
    location: str,
    previous: Optional[str],
    current: str,
    next_text: Optional[str],
) -> str:
    lines = [
        f"[{source_id}]",
        f"文件: {file_name}",
        f"路径: {path}",
        f"位置: {location}",
        "上下文:",
    ]

    if previous and previous.strip():
        lines.append(f"- 上一段: {previous.strip()}")

    lines.append(f"- 当前段: {current.strip()}")

    if next_text and next_text.strip():
        lines.append(f"- 下一段: {next_text.strip()}")

    return "\n".join(lines)


def source_term_overlap(source: RagChunkRecord, terms: Sequence[str]) -> int:
    if not terms:
        return 0

    title_haystack = f"{source.file_name} {source.heading}".lower()
    body_haystack = (
        f"{source.file_name} {source.heading} {source.path} {source.snippet}"
    ).lower()

    score = 0
    for term in terms:
        normalized = term.strip().lower()
        if len(normalized) < 2:
            continue
        if normalized in title_haystack:
            score += 3
        elif normalized in body_haystack:
            score += 1
    return score


def push_unique_query(queries: List[str], seen: Set[str], query: str) -> None:
    cleaned = query.strip()
    if not cleaned:
        return
    normalized = cleaned.lower()
    if normalized in seen:
        return
    seen.add(normalized)
    queries.append(cleaned)


def build_recall_queries(question: str, session_terms: Sequence[str]) -> List[str]:
    queries: List[str] = []
    seen: Set[str] = set()
    push_unique_query(queries, seen, question)

    joined_terms = " ".join(term.strip() for term in session_terms if term and term.strip())
    if joined_terms:
        push_unique_query(queries, seen, f"{question.strip()} {joined_terms}")
        push_unique_query(queries, seen, joined_terms)

    return queries


def qa_candidate_score(candidate: RagRecallCandidate) -> float:
    query_hit_bonus = max(candidate.query_hits - 1, 0) * 0.18
    term_overlap_bonus = min(candidate.term_overlap, 8) * 0.08
    return candidate.best_score + query_hit_bonus + term_overlap_bonus


def collect_recall_candidates(
    store: RagStore,
    question: str,
    scope_paths: Sequence[str],
    settings,
    limit: int,
    session_terms: Sequence[str],
) -> Tuple[Dict[str, RagRecallCandidate], int, List[str]]:
    recall_limit = max(limit, 1) * 3
    recall_limit = max(recall_limit, 20)
    recall_queries = build_recall_queries(question, session_terms)
    print(
        f"[seekmind:rag] qa recall start queries={len(recall_queries)} "
        f"limit={recall_limit} scope_paths={len(scope_paths)}",
        file=sys.stderr,
        flush=True,
    )

    candidates: Dict[str, RagRecallCandidate] = {}
    for query in recall_queries:
        terms = rewrite_query_terms(query)
        rows = store.search_candidate_chunks(terms, scope_paths, recall_limit)
        for row in rows:
            merge_recall_hit(candidates, row, session_terms)

    return candidates, recall_limit, recall_queries


def select_recall_hits(
    candidates: Dict[str, RagRecallCandidate],
    scope_paths: Sequence[str],
    settings,
    limit: int,
) -> List[RagChunkRecord]:
    hits = list(candidates.values())
    hits.sort(key=lambda candidate: qa_candidate_score(candidate), reverse=True)

    selected_hits: List[RagChunkRecord] = []
    seen_chunk_ids: Set[str] = set()
    for candidate in hits:
        if candidate.best_score < getattr(settings, "min_retrieval_score", 0.0):
            continue
        hit = candidate.hit
        if not matches_scope(hit.path, scope_paths):
            continue
        if hit.chunk_id in seen_chunk_ids:
            continue
        seen_chunk_ids.add(hit.chunk_id)
        selected_hits.append(hit)
        if len(selected_hits) >= max(getattr(settings, "context_chunk_limit", limit), 1) * 2:
            break
    return selected_hits


def pack_context_from_hits(
    store: RagStore,
    selected_hits: Sequence[RagChunkRecord],
    settings,
    limit: int,
) -> RagContext:
    sources: List[RagSourceBlock] = []
    selected_source_records: List[RagChunkRecord] = []
    selected_ids_by_path = selected_chunk_ids_by_path(selected_hits)
    consumed_chunk_ids: Set[str] = set()

    for hit in selected_hits:
        if hit.chunk_id in consumed_chunk_ids or len(sources) >= max(getattr(settings, "context_chunk_limit", limit), 1):
            continue

        chunks = store.list_document_chunks(hit.path)
        matched_index = next((index for index, chunk in enumerate(chunks) if chunk.chunk_id == hit.chunk_id), None)
        if matched_index is None:
            consumed_chunk_ids.add(hit.chunk_id)
            previous = None
            current = hit.snippet
            next_text = None
        else:
            selected_ids = selected_ids_by_path.get(hit.path)
            window_start, window_end, merged_current = build_merged_chunk_window(
                chunks,
                matched_index,
                selected_ids,
            )
            for chunk in chunks[window_start : window_end + 1]:
                consumed_chunk_ids.add(chunk.chunk_id)
            previous = chunks[window_start - 1].snippet if window_start > 0 else None
            next_text = chunks[window_end + 1].snippet if window_end + 1 < len(chunks) else None
            current = merged_current

        # 修复：引用编号改为纯数字，前端和校验层统一显示 [1]、[2]...，保留更轻的标注样式。
        source_id = f"{len(sources) + 1}"
        location_label = build_location_label(hit)
        block = build_prompt_block(
            source_id,
            hit.file_name,
            hit.path,
            location_label,
            previous,
            current,
            next_text,
        )
        sources.append(
            RagSourceBlock(
                source=RagSource(
                    source_id=source_id,
                    chunk_id=hit.chunk_id,
                    file_name=hit.file_name,
                    path=hit.path,
                    ext=hit.ext,
                    title_path=hit.heading,
                    heading=hit.heading,
                    paragraph=hit.paragraph,
                    page=hit.page,
                    snippet=hit.snippet,
                    score=hit.score,
                    rank_reason="Python RAG recall",
                    preview_blocks=[],
                ),
                block=block,
            )
        )
        selected_source_records.append(hit)

    # 修复：RAG 证据装配阶段需要回填切片的结构化块，调用的是现有的 for_chunks 查询方法。
    preview_blocks_by_chunk_id = store.fetch_preview_blocks_for_chunks(selected_source_records)
    for source_block in sources:
        source_block.source.preview_blocks = preview_blocks_by_chunk_id.get(source_block.source.chunk_id, [])

    retrieval = RagRetrieval(
        search_mode="python_rag_sqlite",
        candidate_count=len(selected_hits),
        selected_count=len(sources),
        semantic_enabled=False,
        semantic_fallback=False,
        semantic_fallback_reason="",
    )
    return RagContext(sources=sources, retrieval=retrieval)


def merge_recall_hit(
    candidates: Dict[str, RagRecallCandidate],
    hit: RagChunkRecord,
    session_terms: Sequence[str],
) -> None:
    term_overlap = source_term_overlap(hit, session_terms)
    existing = candidates.get(hit.chunk_id)
    if existing is not None:
        existing.query_hits += 1
        existing.term_overlap = max(existing.term_overlap, term_overlap)
        if hit.score > existing.best_score:
            existing.best_score = hit.score
            existing.hit = hit
        return

    candidates[hit.chunk_id] = RagRecallCandidate(
        hit=hit,
        query_hits=1,
        best_score=hit.score,
        term_overlap=term_overlap,
    )


def selected_chunk_ids_by_path(hits: Sequence[RagChunkRecord]) -> Dict[str, Set[str]]:
    result: Dict[str, Set[str]] = {}
    for hit in hits:
        result.setdefault(hit.path, set()).add(hit.chunk_id)
    return result


def build_merged_chunk_window(
    chunks: Sequence[RagChunkRecord],
    matched_index: int,
    selected_ids: Optional[Set[str]],
) -> Tuple[int, int, str]:
    if not selected_ids:
        return (
            matched_index,
            matched_index,
            chunks[matched_index].snippet if matched_index < len(chunks) else "",
        )

    start = matched_index
    while start > 0 and chunks[start - 1].chunk_id in selected_ids:
        start -= 1

    end = matched_index
    while end + 1 < len(chunks) and chunks[end + 1].chunk_id in selected_ids:
        end += 1

    if start == end:
        merged = chunks[matched_index].snippet
    else:
        merged = "\n".join(
            f"连续段落 {index + 1}: {chunk.snippet.strip()}"
            for index, chunk in enumerate(chunks[start : end + 1])
        )

    return start, end, merged


def build_session_terms(session_context: str) -> List[str]:
    if not session_context.strip():
        return []

    stop_terms = {
        "什么",
        "怎么",
        "如何",
        "是否",
        "这个",
        "那个",
        "它的",
        "它",
        "以及",
        "问题",
        "答案",
        "来源",
        "文档",
        "内容",
        "可以",
        "已经",
    }

    terms: List[str] = []
    seen: Set[str] = set()
    for token in normalize_query(session_context):
        normalized = token.strip().lower()
        if (
            not normalized
            or len(normalized) < 2
            or normalized in stop_terms
            or normalized in seen
        ):
            continue
        seen.add(normalized)
        terms.append(normalized)
        if len(terms) >= 8:
            break
    return terms


def build_qa_context(
    store: RagStore,
    question: str,
    scope_paths: Sequence[str],
    settings,
    limit: int,
    session_terms: Sequence[str],
    emit_progress=None,
) -> RagContext:
    candidates, _, _ = collect_recall_candidates(
        store=store,
        question=question,
        scope_paths=scope_paths,
        settings=settings,
        limit=limit,
        session_terms=session_terms,
    )
    selected_hits = select_recall_hits(
        candidates=candidates,
        scope_paths=scope_paths,
        settings=settings,
        limit=limit,
    )
    candidate_count = len(candidates)
    context = pack_context_from_hits(
        store=store,
        selected_hits=selected_hits,
        settings=settings,
        limit=limit,
    )
    print(
        f"[seekmind:rag] qa recall done candidates={candidate_count} selected_sources={len(context.sources)}",
        file=sys.stderr,
        flush=True,
    )
    context.retrieval.candidate_count = candidate_count
    return context
