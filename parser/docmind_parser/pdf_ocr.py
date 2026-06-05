"""
@author MorningSun
@CreatedDate 2026/06/05
@Description PDF OCR 队列任务定义与扫描候选判定辅助。
"""

from __future__ import annotations

from typing import Optional

from .models import PdfOcrTask


def build_pdf_ocr_task(page_index: int, page_text: str, meaningful: bool, noisy: bool) -> Optional[PdfOcrTask]:
    if page_text and meaningful and not noisy:
        return None

    if not page_text:
        return PdfOcrTask(
            page_index=page_index,
            reason="no_text_layer",
            message=f"第 {page_index} 页无文本层（扫描件），加入 OCR 队列",
            warning="PDF 无文本层",
        )

    if noisy:
        return PdfOcrTask(
            page_index=page_index,
            reason="noisy_text_layer",
            message=f"第 {page_index} 页文本层疑似乱码，加入 OCR 队列",
            warning="PDF 文本层疑似噪声",
        )

    if not meaningful:
        return PdfOcrTask(
            page_index=page_index,
            reason="meaningless_text_layer",
            message=f"第 {page_index} 页文本为乱码（{len(page_text)} 字符），加入 OCR 队列",
            warning="pypdf 提取文本不可用",
        )

    return None
