"""
@author MorningSun
@CreatedDate 2026/06/03
@Description Python sidecar RAG 的引用校验与提示生成。
"""

from __future__ import annotations

import re
from typing import Iterable, List, Optional, Sequence


_CITATION_RE = re.compile(r"\[(S\d+)\]")


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
    lowered = sentence.lower()
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


def build_citation_warning(answer: str, valid_ids: Sequence[str]) -> Optional[str]:
    sentences = split_sentences(answer)
    if not sentences:
        return None

    factual_sentences = [sentence for sentence in sentences if sentence_is_factual(sentence)]
    if not factual_sentences:
        return None

    missing = [
        sentence
        for sentence in factual_sentences
        if not sentence_has_valid_citation(sentence, valid_ids)
    ]
    if not missing:
        return None

    if len(missing) == len(factual_sentences):
        return f"答案有 {len(missing)} 个事实句缺少有效来源标注，建议重新生成。"

    return f"答案有 {len(missing)} 个句子缺少来源标注，建议重新生成。"

