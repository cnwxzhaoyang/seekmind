"""
@author MorningSun
@CreatedDate 2026/06/03
@Description Python sidecar RAG 的 OpenAI-compatible 模型调用。
"""

from __future__ import annotations

import json
from typing import Iterator, Optional
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen


def _format_request_error(prefix: str, error: Exception) -> str:
    message = str(error)
    lowered = message.lower()
    if "timed out" in lowered:
        return f"{prefix}: 模型服务响应超时"
    if isinstance(error, URLError):
        return f"{prefix}: 无法连接模型服务"
    return f"{prefix}: {message}"


def _extract_choice_text(choice: object) -> str:
    if not isinstance(choice, dict):
        return ""
    message = choice.get("message") or {}
    if not isinstance(message, dict):
        message = {}
    # 修复：不同兼容实现会把正文放在 content / reasoning_content / response / text 等字段里，
    # 只读 content 会把 Ollama / DeepSeek 一类模型误判成“模型返回为空”。
    for candidate in (
        message.get("content"),
        message.get("reasoning_content"),
        message.get("reasoning"),
        message.get("thinking"),
        message.get("response"),
        message.get("answer"),
        message.get("output"),
        message.get("completion"),
        message.get("generated_text"),
        choice.get("text"),
        choice.get("content"),
        choice.get("response"),
        choice.get("answer"),
        choice.get("output"),
        choice.get("completion"),
        choice.get("generated_text"),
    ):
        text = str(candidate or "").strip()
        if text:
            return text
    return ""


def _extract_response_text(parsed: object) -> str:
    if not isinstance(parsed, dict):
        return ""
    choices = parsed.get("choices") or []
    for choice in choices:
        content = _extract_choice_text(choice)
        if content:
            return content

    for candidate in (
        parsed.get("response"),
        parsed.get("text"),
        parsed.get("answer"),
        parsed.get("output"),
        parsed.get("completion"),
        parsed.get("generated_text"),
    ):
        text = str(candidate or "").strip()
        if text:
            return text

    message = parsed.get("message") or {}
    if isinstance(message, dict):
        for candidate in (
            message.get("content"),
            message.get("reasoning_content"),
            message.get("reasoning"),
            message.get("thinking"),
            message.get("response"),
            message.get("answer"),
            message.get("output"),
            message.get("completion"),
            message.get("generated_text"),
        ):
            text = str(candidate or "").strip()
            if text:
                return text
    return ""


def ask_model(
    base_url: str,
    api_key: str,
    model: str,
    question: str,
    prompt: str,
    temperature: float,
    max_output_tokens: int,
    timeout: int = 120,
) -> str:
    if not base_url.strip():
        raise RuntimeError("模型服务地址未配置")
    if not model.strip():
        raise RuntimeError("模型名称未配置")

    url = f"{base_url.rstrip('/')}/chat/completions"
    payload = {
        "model": model,
        "messages": [
            {"role": "system", "content": prompt},
            {"role": "user", "content": question},
        ],
        "temperature": temperature,
        "max_tokens": max_output_tokens,
        "stream": False,
    }
    body = json.dumps(payload, ensure_ascii=False).encode("utf-8")
    headers = {"Content-Type": "application/json"}
    if api_key.strip():
        headers["Authorization"] = f"Bearer {api_key.strip()}"

    request = Request(url, data=body, headers=headers, method="POST")
    try:
        with urlopen(request, timeout=timeout) as response:
            raw = response.read().decode("utf-8", errors="replace")
    except HTTPError as error:
        detail = error.read().decode("utf-8", errors="replace") if error.fp else ""
        raise RuntimeError(f"模型请求失败: {error.code} {detail}".strip()) from error
    except Exception as error:  # noqa: BLE001
        raise RuntimeError(_format_request_error("模型请求失败", error)) from error

    try:
        parsed = json.loads(raw)
    except Exception as error:  # noqa: BLE001
        raise RuntimeError(f"模型响应解析失败: {error}") from error

    content = _extract_response_text(parsed)
    if content:
        return content

    # 修复：空返回时把响应结构写进报错，方便快速定位是兼容字段差异还是模型服务本身没吐内容。
    keys = sorted(parsed.keys()) if isinstance(parsed, dict) else []
    raise RuntimeError(f"模型返回为空: keys={keys}")


def ask_model_text(
    base_url: str,
    api_key: str,
    model: str,
    question: str,
    prompt: str,
    temperature: float,
    max_output_tokens: int,
    timeout: int = 120,
) -> str:
    stream_parts = []
    try:
        for chunk in ask_model_stream(
            base_url=base_url,
            api_key=api_key,
            model=model,
            question=question,
            prompt=prompt,
            temperature=temperature,
            max_output_tokens=max_output_tokens,
            timeout=timeout,
        ):
            stream_parts.append(chunk)
        streamed_text = "".join(stream_parts).strip()
        if streamed_text:
            return streamed_text
    except Exception:
        # 修复：流式路径偶发解析失败时，保留非流式兜底，避免 judge / repair 因临时协议差异直接中断。
        pass

    return ask_model(
        base_url=base_url,
        api_key=api_key,
        model=model,
        question=question,
        prompt=prompt,
        temperature=temperature,
        max_output_tokens=max_output_tokens,
        timeout=timeout,
    )


def ask_model_stream(
    base_url: str,
    api_key: str,
    model: str,
    question: str,
    prompt: str,
    temperature: float,
    max_output_tokens: int,
    timeout: int = 120,
) -> Iterator[str]:
    if not base_url.strip():
        raise RuntimeError("模型服务地址未配置")
    if not model.strip():
        raise RuntimeError("模型名称未配置")

    url = f"{base_url.rstrip('/')}/chat/completions"
    payload = {
        "model": model,
        "messages": [
            {"role": "system", "content": prompt},
            {"role": "user", "content": question},
        ],
        "temperature": temperature,
        "max_tokens": max_output_tokens,
        "stream": True,
    }
    body = json.dumps(payload, ensure_ascii=False).encode("utf-8")
    headers = {"Content-Type": "application/json"}
    if api_key.strip():
        headers["Authorization"] = f"Bearer {api_key.strip()}"

    request = Request(url, data=body, headers=headers, method="POST")
    try:
        with urlopen(request, timeout=timeout) as response:
            for raw_line in response:
                line = raw_line.decode("utf-8", errors="replace").strip()
                if not line:
                    continue
                if line.startswith("data:"):
                    line = line.removeprefix("data:").strip()
                if not line or line == "[DONE]":
                    if line == "[DONE]":
                        break
                    continue

                try:
                    parsed = json.loads(line)
                except Exception:  # noqa: BLE001
                    continue

                choices = parsed.get("choices") or []
                for choice in choices:
                    delta = choice.get("delta") or choice.get("message") or {}
                    chunk = str(
                        delta.get("content")
                        or delta.get("reasoning_content")
                        or ""
                    )
                    if chunk:
                        yield chunk
    except HTTPError as error:
        detail = error.read().decode("utf-8", errors="replace") if error.fp else ""
        raise RuntimeError(f"模型请求失败: {error.code} {detail}".strip()) from error
    except Exception as error:  # noqa: BLE001
        raise RuntimeError(_format_request_error("模型请求失败", error)) from error
