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

    choices = parsed.get("choices") or []
    for choice in choices:
        message = choice.get("message") or {}
        content = str(message.get("content") or "").strip()
        if content:
            return content

    raise RuntimeError("模型返回为空")


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
