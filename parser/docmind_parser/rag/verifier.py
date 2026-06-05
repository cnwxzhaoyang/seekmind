"""
@author MorningSun
@CreatedDate 2026/06/03
@Description Python sidecar RAG 的引用校验与提示生成。
"""

from __future__ import annotations

import re
from typing import List, Optional, Sequence


_CITATION_RE = re.compile(r"\[(\d+)\]")
_TERM_RE = re.compile(r"[A-Za-z0-9_]+|[\u4e00-\u9fff]+")
_NON_FACTUAL_PREFIXES = (
    "好的",
    "当然",
    "下面",
    "以下",
    "接下来",
    "简单来说",
    "简单讲",
    "简单地说",
    "总之",
    "总结",
    "概括",
    "首先",
    "然后",
    "最后",
    "因此",
    "所以",
    "另外",
    "补充一下",
    "先说",
    "先看",
    "我们来",
    "让我们",
    "以下是",
    "这是",
    "这是一种",
)


def extract_citation_ids(answer: str) -> List[str]:
    seen: List[str] = []
    for match in _CITATION_RE.finditer(answer or ""):
        citation_id = match.group(1)
        if citation_id not in seen:
            seen.append(citation_id)
    return seen


def split_sentences(answer: str) -> List[str]:
    if not answer.strip():
        return []

    parts = re.split(r"(?<=[。！？.!?])\s+|\n+", answer)
    sentences = [part.strip() for part in parts if part and part.strip()]
    return sentences


def sentence_is_factual(sentence: str) -> bool:
    stripped = (sentence or "").strip()
    lowered = stripped.lower()
    if not stripped:
        return False

    if len(stripped) <= 20 and stripped.endswith(("：", ":")):
        return False

    compact = re.sub(r"\s+", "", stripped)
    if compact in {"目的：", "形式：", "组成部分：", "核心思想：", "特点：", "作用：", "好处：", "说明：", "结论：", "定义："}:
        return False

    for prefix in _NON_FACTUAL_PREFIXES:
        if stripped.startswith(prefix):
            remainder = stripped[len(prefix) :].lstrip("，,。:： ").strip()
            if not remainder or len(remainder) <= 12:
                return False
            if prefix in {"好的", "当然", "下面", "以下", "接下来", "简单来说", "简单讲", "简单地说", "总之", "总结", "概括", "我们来", "让我们"}:
                return True if re.search(r"\d|[A-Za-z]|[一二三四五六七八九十]+", remainder) else False

    if re.fullmatch(r"[A-Za-z0-9\u4e00-\u9fff]+[：:]", compact):
        return False

    markers = (
        "需要",
        "必须",
        "配置",
        "执行",
        "创建",
        "修改",
        "删除",
        "支持",
        "不支持",
        "步骤",
        "建议",
        "因为",
        "所以",
        "首先",
        "然后",
        "最后",
        "路径",
        "命令",
        "版本",
        "端口",
        "参数",
        "表",
    )
    if any(marker in lowered for marker in markers):
        return True
    if re.search(r"\d", sentence):
        return True
    if "/" in sentence or "\\" in sentence or "`" in sentence:
        return True
    return False


def normalize_text(value: str) -> str:
    return re.sub(r"\s+", "", (value or "").lower())


def sentence_terms(sentence: str) -> List[str]:
    raw_terms = _TERM_RE.findall(sentence or "")
    terms: List[str] = []
    for term in raw_terms:
        cleaned = term.strip().lower()
        if not cleaned:
            continue
        if len(cleaned) == 1:
            continue
        if cleaned not in terms:
            terms.append(cleaned)
    return terms


def sentence_is_supported_by_source(sentence: str, source_texts: Sequence[str]) -> bool:
    cleaned_sentence = re.sub(r"\[(\d+)\]", "", sentence or "").strip()
    if not cleaned_sentence:
        return True

    normalized_sentence = normalize_text(cleaned_sentence)
    if not normalized_sentence:
        return True

    if len(normalized_sentence) >= 10 and any(
        normalized_sentence in normalize_text(source_text) for source_text in source_texts
    ):
        return True

    terms = sentence_terms(cleaned_sentence)
    if not terms:
        return False

    for source_text in source_texts:
        normalized_source = normalize_text(source_text)
        matched_terms = [term for term in terms if term in normalized_source]
        if len(matched_terms) >= 2:
            return True
        if len(matched_terms) == 1 and len(cleaned_sentence) <= 18:
            return True
    return False


def sentence_has_valid_citation(sentence: str, valid_ids: Sequence[str]) -> bool:
    if not sentence.strip():
        return True
    citations = extract_citation_ids(sentence)
    if not citations:
        return not sentence_is_factual(sentence)
    if not valid_ids:
        return False
    valid_set = set(valid_ids)
    return any(citation_id in valid_set for citation_id in citations)


def build_citation_warning(
    answer: str,
    valid_ids: Sequence[str],
    source_texts: Optional[Sequence[str]] = None,
) -> Optional[str]:
    sentences = split_sentences(answer)
    if not sentences:
        return None

    factual_sentences = [sentence for sentence in sentences if sentence_is_factual(sentence)]
    if not factual_sentences:
        return None

    normalized_sources = [text for text in (source_texts or []) if text and text.strip()]

    missing = [
        sentence
        for sentence in factual_sentences
        if not sentence_has_valid_citation(sentence, valid_ids)
        and not sentence_is_supported_by_source(sentence, normalized_sources)
    ]
    if not missing:
        return None

    if len(missing) == len(factual_sentences):
        return f"答案有 {len(missing)} 个事实句缺少有效来源标注，建议重新生成。"

    return f"答案有 {len(missing)} 个句子缺少来源标注，建议重新生成。"
