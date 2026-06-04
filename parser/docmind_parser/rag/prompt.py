"""
@author MorningSun
@CreatedDate 2026/06/03
@Description Python sidecar RAG 的 prompt 组装。
"""

from __future__ import annotations

from typing import Optional

from .retrieval import RagContext


def build_system_prompt(
    context: RagContext,
    session_context: Optional[str] = None,
    relation_hint: Optional[str] = None,
    repair_hint: Optional[str] = None,
) -> str:
    prompt = (
        "你是 DocMind 的本地文档问答引擎。只能基于给定来源回答，不能使用外部知识补全事实。"
        "如果来源不足，直接说明无法从当前文档判断。回答要简短、具体、可回溯。"
        "不要编造新的来源编号，不要输出与来源无关的结论。"
        "如果能回答，请用与用户问题相同的语言输出，并尽量把结论控制在 3 到 6 句以内。"
        "答案正文中的每个结论句末尾都必须带来源标注，例如 [S1]。如果一句结论依赖多个来源，可以同时写多个标注，例如 [S1][S3]。"
        "不要把来源标注单独放到最后的参考列表里，标注要跟在正文句子后面。\n\n"
        "可用来源如下：\n"
    )

    if session_context and session_context.strip():
        prompt += "\n最近对话上下文（仅用于理解指代，不可当作事实来源）：\n"
        prompt += session_context.strip()
        prompt += "\n\n"

    if relation_hint and relation_hint.strip():
        prompt += "\n问句重写提示（仅用于理解当前问题，不可当作事实来源）：\n"
        prompt += relation_hint.strip()
        prompt += "\n\n"

    if repair_hint and repair_hint.strip():
        # 修复：修复阶段需要更明确地约束模型只重写可证事实，避免把上一版无引用句子原样带回。
        prompt += "\n答案修复提示（仅用于重写，不可当作事实来源）：\n"
        prompt += repair_hint.strip()
        prompt += "\n\n"

    for source in context.sources:
        prompt += "\n"
        prompt += source.block
        prompt += "\n\n"

    prompt += (
        "输出要求：\n"
        "1. 只输出最终答案正文，不要输出 JSON。\n"
        "2. 不要列出你没有使用的来源。\n"
        "3. 如果当前问题包含“这两者”“二者”“它们”“关系”等指代，必须先依据最近对话确定指代对象；不要把新检索到但不属于这些对象的来源当作比较对象。\n"
        "4. 如果无法回答，直接说明证据不足，并说明建议补充哪些文档类型或关键词。\n"
    )

    return prompt


def build_judge_prompt(
    context: RagContext,
    answer: str,
    warning: Optional[str] = None,
) -> str:
    prompt = (
        "你是 DocMind 的问答证据校验器。你的任务不是回答问题，而是判断当前答案是否足够被给定来源支撑。"
        "如果答案里存在大量没有来源支撑的事实句，或者结论明显超出了来源范围，就应当要求重写。"
        "如果只是少量辅助句、过渡句没有标注，但核心事实是可证的，可以不要求重写。"
        "请只输出 JSON，不要输出多余文本。\n\n"
        "JSON 结构必须是：\n"
        "{"
        "\"should_repair\": true/false, "
        "\"reason\": \"简短原因\", "
        "\"confidence\": 0.0-1.0"
        "}\n\n"
    )

    if warning and warning.strip():
        prompt += "规则校验提示：\n"
        prompt += warning.strip()
        prompt += "\n\n"

    prompt += "待校验答案：\n"
    prompt += answer.strip()
    prompt += "\n\n"
    prompt += "可用来源：\n"
    for source in context.sources:
        prompt += "\n"
        prompt += source.block
        prompt += "\n\n"

    prompt += (
        "判定规则：\n"
        "1. should_repair=true 仅在核心事实缺少证据、结论与来源冲突、或答案明显依赖外部知识时使用。\n"
        "2. should_repair=false 可以接受少量无引用辅助语句，但不能接受核心事实失支撑。\n"
        "3. reason 只写一句话，直接说明原因。\n"
    )
    return prompt
