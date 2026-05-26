from __future__ import annotations

import html
import html.parser
import os
import re
import subprocess
import zipfile
from functools import lru_cache
from dataclasses import dataclass
from pathlib import Path
from urllib.parse import urlparse
from typing import Callable, Iterable, List, Optional, Sequence, Tuple
from xml.etree import ElementTree

from .models import (
    EmbeddingResponse,
    EmbeddingStatus,
    ParsedBlock,
    ParsedChunk,
    ParsedDocument,
    ParserStreamMessage,
    ParserError,
    ParserOptions,
)

SUPPORTED_EXTENSIONS = {"txt", "md", "markdown", "html", "htm", "doc", "docx", "pdf"}
DEFAULT_EMBEDDING_MODEL = "BAAI/bge-small-zh-v1.5"
EMBEDDING_DIMENSION_HINTS = {
    "BAAI/bge-small-zh-v1.5": 512,
    "BAAI/bge-small-en-v1.5": 384,
    "sentence-transformers/all-MiniLM-L6-v2": 384,
    "jinaai/jina-embeddings-v2-base-zh": 768,
}


@dataclass
class Block:
    kind: str
    text: str
    heading: Optional[str] = None
    page_no: Optional[int] = None
    section: Optional[str] = None
    level: Optional[int] = None
    markdown: Optional[str] = None
    html: Optional[str] = None
    asset_path: Optional[str] = None
    alt_text: Optional[str] = None
    caption: Optional[str] = None
    ocr_text: Optional[str] = None


@dataclass
class FastEmbedRuntime:
    available: bool
    provider: str
    model_name: str
    model_path: str
    dimension: int
    message: str
    model: Optional[object] = None


class HtmlBlockExtractor(html.parser.HTMLParser):
    BLOCK_TAGS = {
        "p",
        "li",
        "h1",
        "h2",
        "h3",
        "h4",
        "h5",
        "h6",
        "pre",
        "blockquote",
        "td",
        "th",
    }

    def __init__(self, base_path: Optional[Path] = None) -> None:
        super().__init__()
        self.base_path = base_path
        self.blocks: List[Block] = []
        self.title: Optional[str] = None
        self._stack: List[str] = []
        self._buffer: List[str] = []
        self._heading_stack: List[tuple[int, str]] = []
        self._current_row: List[str] = []
        self._table_rows: List[List[str]] = []
        self._in_table = False
        self._row_in_progress = False

    def handle_starttag(self, tag: str, attrs):  # type: ignore[override]
        attrs_map = {key.lower(): value for key, value in attrs}
        if tag == "img":
            self._flush()
            self._emit_image(attrs_map)
            return
        self._stack.append(tag)
        if tag == "title":
            self._flush()
            return
        if tag == "table":
            self._flush()
            self._table_rows = []
            self._in_table = True
            return
        if tag in self.BLOCK_TAGS:
            self._flush()
            if tag in {"td", "th"}:
                self._current_row.append("")
                self._row_in_progress = True

    def handle_endtag(self, tag: str):  # type: ignore[override]
        if tag == "title":
            self._flush_title()
        elif tag == "table":
            self._flush_row()
            self._flush_table()
        elif tag in {"tr"}:
            self._flush_row()
        elif tag in self.BLOCK_TAGS:
            self._flush()
        if self._stack:
            while self._stack and self._stack[-1] != tag:
                self._stack.pop()
            if self._stack and self._stack[-1] == tag:
                self._stack.pop()

    def handle_data(self, data: str):  # type: ignore[override]
        if "title" in self._stack:
            self._buffer.append(data)
            return
        if self._stack and self._stack[-1] in {"td", "th"} and self._current_row:
            self._current_row[-1] += data
            return
        if any(tag in self._stack for tag in self.BLOCK_TAGS):
            self._buffer.append(data)

    def _emit_image(self, attrs: dict[str, str]) -> None:
        src = normalize_whitespace(attrs.get("src", ""))
        alt = normalize_whitespace(attrs.get("alt", ""))
        caption = normalize_whitespace(attrs.get("title", ""))
        asset_path = resolve_media_src(src, self.base_path)
        label = alt or caption or image_label_from_path(asset_path) or "image"
        self.blocks.append(
            Block(
                kind="image",
                text=label,
                heading=self._current_heading_path(),
                asset_path=asset_path or None,
                alt_text=alt or None,
                caption=caption or None,
                html=build_img_html(asset_path or src, alt, caption),
            )
        )

    def _flush_title(self) -> None:
        text = normalize_whitespace("".join(self._buffer))
        self._buffer.clear()
        text = strip_noise_paragraph(text)
        if text and self.title is None:
            self.title = text

    def _flush_row(self) -> None:
        if self._current_row:
            row = [normalize_whitespace(cell) for cell in self._current_row if normalize_whitespace(cell)]
            if row:
                self._table_rows.append(row)
        self._current_row.clear()
        self._row_in_progress = False

    def _flush_table(self) -> None:
        if not self._table_rows:
            self._in_table = False
            return
        text, markdown, html_table = table_rows_to_formats(self._table_rows)
        if text:
            self.blocks.append(
                Block(
                    kind="table",
                    text=text,
                    heading=self._current_heading_path(),
                    markdown=markdown,
                    html=html_table,
                )
            )
        self._table_rows = []
        self._in_table = False

    def _flush(self) -> None:
        text = normalize_whitespace("".join(self._buffer))
        self._buffer.clear()
        if not text:
            return
        current_tag = self._stack[-1] if self._stack else None
        if current_tag and current_tag.startswith("h") and len(current_tag) == 2 and current_tag[1].isdigit():
            level = int(current_tag[1])
            self._push_heading(level, text)
            self.blocks.append(Block(kind="heading", text=text, level=level, heading=self._current_heading_path()))
            return
        html_kind_map = {"li": "list", "p": "paragraph", "pre": "code", "blockquote": "quote", "td": "table", "th": "table"}
        kind = html_kind_map.get(current_tag or "", current_tag or "text")
        self.blocks.append(Block(kind=kind, text=text, heading=self._current_heading_path()))
        return

    def _push_heading(self, level: int, text: str) -> None:
        while self._heading_stack and self._heading_stack[-1][0] >= level:
            self._heading_stack.pop()
        self._heading_stack.append((level, text))

    def _current_heading_path(self) -> Optional[str]:
        if not self._heading_stack:
            return None
        return " > ".join(heading for _, heading in self._heading_stack)


def resolve_media_src(src: str, base_path: Optional[Path]) -> str:
    cleaned = normalize_whitespace(src).strip("<>")
    if not cleaned:
        return ""
    parsed = urlparse(cleaned)
    if parsed.scheme in {"http", "https", "data", "blob"}:
        return cleaned
    if parsed.scheme == "file":
        return parsed.path or cleaned
    candidate = Path(cleaned)
    if candidate.is_absolute():
        return str(candidate)
    if base_path is not None:
        return str((base_path / candidate).resolve())
    return str(candidate.resolve())


def image_label_from_path(asset_path: str) -> str:
    if not asset_path:
        return ""
    parsed = urlparse(asset_path)
    if parsed.scheme in {"http", "https", "data", "blob"}:
        candidate = parsed.path or asset_path
    elif parsed.scheme == "file":
        candidate = parsed.path or asset_path
    else:
        candidate = asset_path
    return Path(candidate).name


def build_img_html(src: str, alt: str, caption: str) -> str:
    src_attr = html.escape(src, quote=True)
    alt_attr = html.escape(alt, quote=True)
    title_attr = html.escape(caption, quote=True)
    title_part = f' title="{title_attr}"' if title_attr else ""
    return f'<img src="{src_attr}" alt="{alt_attr}"{title_part} />'


def parse_markdown_image(raw_line: str, base_path: Path, heading: Optional[str]) -> Optional[Block]:
    pattern = re.compile(
        r"""^!\[(?P<alt>.*?)\]\((?P<src>[^\s)]+)(?:\s+(?:"(?P<title1>[^"]*)"|'(?P<title2>[^']*)'|(?P<title3>[^)]*)))?\)$"""
    )
    match = pattern.match(raw_line.strip())
    if not match:
        return None
    alt = normalize_whitespace(match.group("alt") or "")
    src = normalize_whitespace(match.group("src") or "")
    caption = normalize_whitespace(match.group("title1") or match.group("title2") or match.group("title3") or "")
    asset_path = resolve_media_src(src, base_path)
    label = alt or caption or image_label_from_path(asset_path) or "image"
    return Block(
        kind="image",
        text=label,
        heading=heading,
        markdown=raw_line.strip(),
        html=build_img_html(asset_path or src, alt, caption),
        asset_path=asset_path or None,
        alt_text=alt or None,
        caption=caption or None,
    )

ProgressEmitter = Callable[[dict], None]


def parse_document(
    path: Path,
    options: ParserOptions,
    emit: Optional[ProgressEmitter] = None,
    request_id: str = "",
) -> ParsedDocument:
    def progress(
        stage: str,
        message: str,
        percent: int = 0,
        current: str = "",
        total: int = 0,
        processed: int = 0,
        warning: Optional[str] = None,
    ) -> None:
        if emit is None:
            return
        emit(
            ParserStreamMessage(
                request_id=request_id,
                kind="event",
                event="progress",
                message=message,
                stage=stage,
                percent=percent,
                current=current,
                total=total,
                processed=processed,
                parser_source="python",
                warning=warning,
            ).to_dict()
        )

    if not path.exists():
        raise parser_error("file_not_found", f"file not found: {path}")
    if not path.is_file():
        raise parser_error("invalid_request", f"not a file: {path}")

    ext = normalize_extension(path.suffix.lower().lstrip("."))
    if ext not in SUPPORTED_EXTENSIONS:
        raise parser_error("unsupported_file_type", f"unsupported file type: {ext}")

    progress("start", f"开始解析 {path.name}", 1, path.name)

    if ext in {"txt", "md", "markdown"}:
        title, blocks = parse_text_like(path, ext)
    elif ext in {"html", "htm"}:
        title, blocks = parse_html(path)
    elif ext == "doc":
        title, blocks = parse_doc(path)
    elif ext == "docx":
        title, blocks = parse_docx(path)
    elif ext == "pdf":
        title, blocks = parse_pdf(path)
    else:
        raise parser_error("unsupported_file_type", f"unsupported file type: {ext}")

    progress("extract", f"已提取 {len(blocks)} 个内容块", 35, path.name, len(blocks), len(blocks))
    blocks = merge_short_blocks(normalize_blocks(blocks))
    progress("normalize", "正在整理内容块", 60, path.name, len(blocks), len(blocks))
    content = "\n\n".join(
        block.text for block in blocks if block.text.strip() and block.section != "frontmatter"
    )
    chunks = build_chunks(blocks, path, options, emit=emit)
    progress("chunk", f"已生成 {len(chunks)} 个切片", 90, path.name, len(chunks), len(chunks))
    if options.max_chunks is not None:
        chunks = chunks[: max(int(options.max_chunks), 0)]

    parsed_blocks = [
        ParsedBlock(
            block_index=index,
            type=block.kind,
            text=block.text,
            heading=block.heading,
            level=block.level,
            page_no=block.page_no,
            markdown=block.markdown,
            html=block.html,
            asset_path=block.asset_path,
            alt_text=block.alt_text,
            caption=block.caption,
            ocr_text=block.ocr_text,
        )
        for index, block in enumerate(blocks, start=1)
        if block.section != "frontmatter" and block.text.strip()
    ]

    progress("done", f"解析完成：{path.name}", 100, path.name, len(chunks), len(chunks))
    return ParsedDocument(
        title=title or path.stem,
        file_type=ext,
        content=content,
        chunks=chunks if options.include_chunks else [],
        blocks=parsed_blocks,
    )


def embedding_status(model_name: Optional[str] = None) -> EmbeddingStatus:
    runtime = get_fastembed_runtime(model_name, eager=False)
    return EmbeddingStatus(
        available=runtime.available,
        provider=runtime.provider,
        model_name=runtime.model_name,
        model_path=runtime.model_path,
        dimension=runtime.dimension,
        message=runtime.message,
    )


def embed_texts(
    texts: Sequence[str],
    model_name: Optional[str] = None,
    emit: Optional[ProgressEmitter] = None,
    request_id: str = "",
) -> EmbeddingResponse:
    runtime = get_fastembed_runtime(model_name, eager=True)
    if not runtime.available or runtime.model is None:
        raise parser_error("embedding_unavailable", runtime.message or "fastembed is not available")

    normalized_texts = [normalize_whitespace(text) for text in texts if normalize_whitespace(text)]
    if not normalized_texts:
        return EmbeddingResponse(vectors=[], status=embedding_status(model_name))

    try:
        vectors: List[List[float]] = []
        total = len(normalized_texts)
        if emit is not None:
            emit(
                ParserStreamMessage(
                    request_id=request_id,
                    kind="event",
                    event="progress",
                    message=f"正在生成 {total} 个语义向量",
                    stage="embedding",
                    percent=0,
                    current="",
                    total=total,
                    processed=0,
                    parser_source="python",
                ).to_dict()
            )

        for index, vector in enumerate(runtime.model.embed(normalized_texts), start=1):
            vectors.append(list(map(float, vector)))
            if emit is not None:
                emit(
                    ParserStreamMessage(
                        request_id=request_id,
                        kind="event",
                        event="progress",
                        message=f"正在生成语义向量 {index}/{total}",
                        stage="embedding",
                        percent=int(round(index / total * 100)),
                        current=f"{index}/{total}",
                        total=total,
                        processed=index,
                        parser_source="python",
                    ).to_dict()
                )

        if emit is not None:
            emit(
                ParserStreamMessage(
                    request_id=request_id,
                    kind="event",
                    event="progress",
                    message="语义向量生成完成",
                    stage="embedding",
                    percent=100,
                    current="",
                    total=total,
                    processed=total,
                    parser_source="python",
                ).to_dict()
            )

        return EmbeddingResponse(vectors=vectors, status=embedding_status(model_name))
    except Exception as exc:  # noqa: BLE001
        raise parser_error("embedding_failed", normalize_embedding_error(exc)) from exc


@lru_cache(maxsize=16)
def get_fastembed_runtime(model_name: Optional[str] = None, eager: bool = True) -> FastEmbedRuntime:
    target_model = (model_name or DEFAULT_EMBEDDING_MODEL).strip() or DEFAULT_EMBEDDING_MODEL
    try:
        from fastembed import TextEmbedding  # type: ignore
    except Exception as exc:  # noqa: BLE001
        return FastEmbedRuntime(
            available=False,
            provider="fastembed",
            model_name=target_model,
            model_path="",
            dimension=0,
            message=f"fastembed is not installed: {exc}",
            model=None,
        )

    if not eager:
        return FastEmbedRuntime(
            available=True,
            provider="fastembed",
            model_name=target_model,
            model_path="",
            dimension=infer_embedding_dimension(None, target_model),
            message="ready (lazy)",
            model=None,
        )

    try:
        model = TextEmbedding(
            model_name=target_model,
            cache_dir=os.environ.get("DOCMIND_FASTEMBED_CACHE_DIR") or None,
        )
        dimension = infer_embedding_dimension(model, target_model)
        return FastEmbedRuntime(
            available=True,
            provider="fastembed",
            model_name=target_model,
            model_path="",
            dimension=dimension,
            message="ready",
            model=model,
        )
    except Exception as exc:  # noqa: BLE001
        message = normalize_embedding_error(exc)
        return FastEmbedRuntime(
            available=False,
            provider="fastembed",
            model_name=target_model,
            model_path="",
            dimension=0,
            message=message,
            model=None,
        )


def infer_embedding_dimension(model: Optional[object], model_name: Optional[str] = None) -> int:
    if model_name:
        hint = EMBEDDING_DIMENSION_HINTS.get(model_name.strip())
        if hint is not None:
            return hint

    if model is None:
        return 384

    for attr_name in ("embedding_size", "dim", "dimension", "ndim"):
        value = getattr(model, attr_name, None)
        if isinstance(value, int) and value > 0:
            return value
        if callable(value):
            try:
                result = value()
                if isinstance(result, int) and result > 0:
                    return result
            except Exception:  # noqa: BLE001
                pass
    return 384


def normalize_embedding_error(exc: Exception) -> str:
    message = str(exc)
    lower = message.lower()
    if "operation timed out" in lower or "timed out" in lower or "timeout" in lower:
        return (
            "embedding 模型下载或加载超时。请先在终端执行 "
            "`npm run semantic:warmup`，网络受限时执行 "
            "`npm run semantic:warmup:mirror`。原始错误: "
            f"{message}"
        )
    return message


def parse_text_like(path: Path, ext: str) -> Tuple[Optional[str], List[Block]]:
    text = path.read_text(encoding="utf-8", errors="ignore").replace("\r\n", "\n")
    if ext == "txt":
        return parse_plain_text(text, path.stem)
    return parse_markdown(text, path.stem, path.parent)


def parse_plain_text(text: str, fallback_title: str) -> Tuple[Optional[str], List[Block]]:
    blocks: List[Block] = []
    paragraphs = split_paragraphs(text)
    title: Optional[str] = None

    for paragraph in paragraphs:
        heading = detect_title_like_line(paragraph)
        if heading and title is None:
            title = heading
        blocks.append(Block(kind="paragraph", text=paragraph, heading=heading or title))

    return title or fallback_title, blocks


def parse_markdown(text: str, fallback_title: str, base_path: Path) -> Tuple[Optional[str], List[Block]]:
    blocks: List[Block] = []
    title: Optional[str] = None
    heading_stack: List[tuple[int, str]] = []
    buffer: List[str] = []
    in_code = False
    code_fence = ""
    code_lang = ""
    table_buffer: List[str] = []

    def heading_path() -> Optional[str]:
        if not heading_stack:
            return None
        return " > ".join(heading for _, heading in heading_stack)

    def flush_buffer(kind: str = "paragraph") -> None:
        nonlocal buffer, title
        text = normalize_whitespace("\n".join(buffer))
        buffer = []
        if text:
            cleaned = strip_noise_paragraph(text)
            if cleaned:
                blocks.append(Block(kind=kind, text=cleaned, heading=heading_path()))
            if title is None:
                title = heading_path() or detect_title_like_line(text) or fallback_title

    def flush_table() -> None:
        nonlocal table_buffer, title
        raw_md = "\n".join(table_buffer) if table_buffer else ""
        table = normalize_whitespace(" | ".join(line.strip(" |") for line in table_buffer))
        table_buffer = []
        if table:
            cleaned = strip_noise_paragraph(table)
            if cleaned:
                blocks.append(Block(kind="table", text=cleaned, heading=heading_path(), markdown=raw_md))
            if title is None:
                title = heading_path() or fallback_title

    def emit_image(raw_line: str) -> bool:
        nonlocal title
        image_block = parse_markdown_image(raw_line, base_path, heading_path())
        if image_block is None:
            return False
        blocks.append(image_block)
        if title is None:
            title = image_block.heading or detect_title_like_line(image_block.text) or fallback_title
        return True

    for raw_line in text.split("\n"):
        line = raw_line.rstrip()
        stripped = line.strip()

        fence = parse_fence_line(stripped)
        if fence:
            if not in_code:
                flush_buffer()
                flush_table()
                in_code = True
                code_fence = fence
                code_lang = stripped[len(fence):].strip()
                buffer.append(stripped)
            else:
                buffer.append(stripped)
                if fence == code_fence:
                    in_code = False
                    code_text = "\n".join(buffer[1:-1])
                    buffer = []
                    if code_text.strip():
                        blocks.append(Block(kind="code", text=code_text, heading=heading_path()))
                        if title is None:
                            title = heading_path() or fallback_title
                    code_fence = ""
                    code_lang = ""
            continue

        if in_code:
            buffer.append(line)
            continue

        heading = parse_markdown_heading(stripped)
        if heading:
            flush_buffer()
            flush_table()
            level = len(re.match(r"^(#{1,6})", stripped).group(1)) if re.match(r"^(#{1,6})", stripped) else 1
            while heading_stack and heading_stack[-1][0] >= level:
                heading_stack.pop()
            heading_stack.append((level, heading))
            blocks.append(Block(kind="heading", text=heading, heading=heading_path(), level=level))
            if title is None:
                title = heading_path() or heading
            continue

        if not stripped:
            flush_buffer()
            flush_table()
            continue

        if is_markdown_table_row(stripped):
            flush_buffer()
            table_buffer.append(stripped)
            continue

        if table_buffer:
            flush_table()

        if emit_image(stripped):
            flush_buffer()
            flush_table()
            continue

        if is_markdown_list_item(stripped):
            flush_buffer()
            cleaned = strip_noise_paragraph(strip_markdown_marker(stripped))
            if cleaned:
                blocks.append(Block(kind="list", text=cleaned, heading=heading_path()))
            if title is None:
                title = heading_path() or detect_title_like_line(stripped) or fallback_title
            continue

        if is_blockquote(stripped):
            flush_buffer()
            cleaned = strip_noise_paragraph(strip_markdown_marker(stripped))
            if cleaned:
                blocks.append(Block(kind="quote", text=cleaned, heading=heading_path()))
            if title is None:
                title = heading_path() or detect_title_like_line(stripped) or fallback_title
            continue

        buffer.append(stripped)

    flush_buffer()
    flush_table()

    if title is None:
        title = fallback_title

    return title, blocks

def parse_html(path: Path) -> Tuple[Optional[str], List[Block]]:
    text = path.read_text(encoding="utf-8", errors="ignore")
    extractor = HtmlBlockExtractor(path.parent)
    extractor.feed(text)
    extractor.close()
    title = extractor.title or infer_title_from_blocks(extractor.blocks) or path.stem
    return title, normalize_blocks(extractor.blocks)


def parse_docx(path: Path) -> Tuple[Optional[str], List[Block]]:
    python_docx_result = parse_docx_with_python_docx(path)
    if python_docx_result is not None:
        return python_docx_result

    with zipfile.ZipFile(path) as archive:
        document_xml = archive.read("word/document.xml")

    ns = {"w": "http://schemas.openxmlformats.org/wordprocessingml/2006/main"}
    root = ElementTree.fromstring(document_xml)
    blocks: List[Block] = []
    title: Optional[str] = None
    heading_stack: List[tuple[int, str]] = []
    seen_body_heading = False

    def heading_path() -> Optional[str]:
        if not heading_stack:
            return None
        return " > ".join(heading for _, heading in heading_stack)

    for element in root.findall(".//w:body/*", ns):
        tag = strip_ns(element.tag)
        if tag == "p":
            style_node = element.find("./w:pPr/w:pStyle", ns)
            style = style_node.attrib.get(f"{{{ns['w']}}}val") if style_node is not None else None
            text = extract_docx_text_from_paragraph(element, ns)
            text = normalize_whitespace(text)
            if not text:
                continue

            if style and style.lower().startswith("heading"):
                match = re.search(r"heading(\d+)", style.lower())
                level = int(match.group(1)) if match else 1
                if title is None:
                    title = text
                    blocks.append(Block(kind="heading", text=text, level=level, section="frontmatter"))
                    continue
                while heading_stack and heading_stack[-1][0] >= level:
                    heading_stack.pop()
                heading_stack.append((level, text))
                seen_body_heading = True
                blocks.append(Block(kind="heading", text=text, level=level, section="body"))
            else:
                section = "body" if seen_body_heading else "frontmatter"
                blocks.append(Block(kind="paragraph", text=text, heading=heading_path(), section=section))
                if title is None:
                    title = heading_path() or detect_title_like_line(text) or path.stem
        elif tag == "tbl":
            rows = extract_docx_table_rows(element, ns)
            if rows:
                text, markdown, html_table = table_rows_to_formats(rows)
                if text:
                    section = "body" if seen_body_heading else "frontmatter"
                    blocks.append(
                        Block(
                            kind="table",
                            text=text,
                            heading=heading_path(),
                            section=section,
                            markdown=markdown,
                            html=html_table,
                        )
                    )
                    if title is None:
                        title = heading_path() or path.stem

    return title or path.stem, blocks


def parse_doc(path: Path) -> Tuple[Optional[str], List[Block]]:
    text = extract_doc_text_with_textutil(path)
    blocks = [Block(kind="paragraph", text=paragraph) for paragraph in split_paragraphs(text)]
    return None, blocks


def parse_docx_with_python_docx(path: Path) -> Optional[Tuple[Optional[str], List[Block]]]:
    try:
        from docx import Document  # type: ignore
        from docx.document import Document as DocxDocument  # type: ignore
        from docx.table import Table  # type: ignore
        from docx.text.paragraph import Paragraph  # type: ignore
    except Exception:
        return None

    try:
        document = Document(str(path))
    except Exception:
        return None

    if not isinstance(document, DocxDocument):
        return None

    blocks: List[Block] = []
    title: Optional[str] = None
    heading_stack: List[tuple[int, str]] = []
    seen_body_heading = False

    def heading_path() -> Optional[str]:
        if not heading_stack:
            return None
        return " > ".join(heading for _, heading in heading_stack)

    for child in document.element.body.iterchildren():
        if child.tag.endswith("}p"):
            paragraph = Paragraph(child, document)
            style_name = getattr(getattr(paragraph, "style", None), "name", "") or ""
            text = clean_docx_text(paragraph.text)
            if not text:
                continue

            if style_name.lower().startswith("heading"):
                match = re.search(r"heading\s*(\d+)", style_name.lower())
                level = int(match.group(1)) if match else 1
                if title is None:
                    title = text
                    blocks.append(Block(kind="heading", text=text, level=level, section="frontmatter"))
                    continue
                while heading_stack and heading_stack[-1][0] >= level:
                    heading_stack.pop()
                heading_stack.append((level, text))
                seen_body_heading = True
                blocks.append(Block(kind="heading", text=text, level=level, section="body"))
            else:
                section = "body" if seen_body_heading else "frontmatter"
                blocks.append(Block(kind="paragraph", text=text, heading=heading_path(), section=section))
                if title is None:
                    title = heading_path() or detect_title_like_line(text) or path.stem

        elif child.tag.endswith("}tbl"):
            table = Table(child, document)
            rows: List[List[str]] = []
            for row in table.rows:
                cells = [clean_docx_text(cell.text) for cell in row.cells]
                normalized_cells = [cell for cell in cells if cell]
                if not normalized_cells:
                    continue
                row_text = normalize_whitespace(" | ".join(normalized_cells))
                if looks_like_docx_xml_noise(row_text):
                    continue
                rows.append(normalized_cells)
            if rows:
                text, markdown, html_table = table_rows_to_formats(rows)
                section = "body" if seen_body_heading else "frontmatter"
                blocks.append(
                    Block(
                        kind="table",
                        text=text,
                        heading=heading_path(),
                        section=section,
                        markdown=markdown,
                        html=html_table,
                    )
                )
                if title is None:
                    title = heading_path() or path.stem

    if not blocks:
        return None

    return title or path.stem, normalize_blocks(blocks)


def extract_doc_text_with_textutil(path: Path) -> str:
    try:
        result = subprocess.run(
            ["/usr/bin/textutil", "-stdout", "-convert", "txt", str(path)],
            check=False,
            capture_output=True,
            text=True,
        )
    except Exception as exc:  # noqa: BLE001
        raise parser_error("parse_failed", f"textutil failed: {exc}") from exc

    if result.returncode != 0:
        message = result.stderr.strip() or result.stdout.strip() or str(path)
        raise parser_error("parse_failed", f"textutil failed: {message}")

    return normalize_whitespace(result.stdout)


def parse_pdf(path: Path) -> Tuple[Optional[str], List[Block]]:
    title: Optional[str] = None
    blocks: List[Block] = []

    try:
        from pypdf import PdfReader  # type: ignore

        reader = PdfReader(str(path))
        for page_index, page in enumerate(reader.pages, start=1):
            page_text = normalize_whitespace((page.extract_text() or "").replace("\x0c", "\n\n"))
            if not page_text:
                continue

            page_paragraphs = split_paragraphs(page_text)
            for paragraph in page_paragraphs:
                if not paragraph:
                    continue
                if title is None:
                    title = detect_title_like_line(paragraph) or path.stem
                blocks.append(
                    Block(
                        kind="paragraph",
                        text=paragraph,
                        heading=title,
                        page_no=page_index,
                    )
                )

        if blocks:
            return title or path.stem, blocks
    except Exception as exc:  # noqa: BLE001
        parse_error = exc
    else:
        parse_error = None

    pdftotext = _extract_pdf_with_pdftotext(path)
    if pdftotext is not None:
        title, blocks = pdftotext
        return title or path.stem, blocks

    if parse_error is not None:
        raise parser_error("parse_failed", "pdf extraction failed", details=str(parse_error))

    raise parser_error("parse_failed", "pdf extraction failed")


def _extract_pdf_with_pdftotext(path: Path) -> Optional[Tuple[Optional[str], List[Block]]]:
    try:
        result = subprocess.run(
            ["pdftotext", "-layout", "-enc", "UTF-8", str(path), "-"],
            check=False,
            capture_output=True,
            text=True,
        )
    except FileNotFoundError:
        return None

    if result.returncode != 0:
        return None

    raw_text = result.stdout.replace("\r\n", "\n")
    if not raw_text.strip():
        return None

    blocks: List[Block] = []
    title: Optional[str] = None

    for page_index, page_text in enumerate(raw_text.split("\f"), start=1):
        paragraphs = split_paragraphs(page_text.replace("\x0c", "\n\n"))
        for paragraph in paragraphs:
            if not paragraph:
                continue
            if title is None:
                title = detect_title_like_line(paragraph) or path.stem
            blocks.append(
                Block(
                    kind="paragraph",
                    text=paragraph,
                    heading=title,
                    page_no=page_index,
                )
            )

    if not blocks:
        return None

    return title or path.stem, blocks


def build_chunks(
    blocks: Sequence[Block],
    path: Path,
    options: ParserOptions,
    emit: Optional[ProgressEmitter] = None,
) -> List[ParsedChunk]:
    chunks: List[ParsedChunk] = []
    max_chars = max(int(options.max_chunk_chars), 120)
    current_heading: Optional[str] = None
    heading_path: Optional[str] = None
    current_page: Optional[int] = None
    current_section: Optional[str] = None
    buffer: List[str] = []
    buffer_block_indexes: List[int] = []
    buffer_score = 1.0
    order = 1

    def flush_buffer() -> None:
        nonlocal buffer, buffer_block_indexes, order, current_section, buffer_score
        text = normalize_whitespace("\n\n".join(buffer))
        buffer = []
        indexes = sorted(set(buffer_block_indexes)) if buffer_block_indexes else None
        buffer_block_indexes = []
        if not text:
            return
        if path.suffix.lower() == ".docx" and looks_like_docx_cover_chunk(text):
            current_section = None
            buffer_score = 1.0
            return
        chunks.append(
            ParsedChunk(
                heading=heading_path or current_heading or path.stem,
                page_no=current_page,
                text=text,
                order=order,
                score=buffer_score,
                block_indexes=indexes,
            )
        )
        order += 1
        current_section = None
        buffer_score = 1.0

    for block_index, block in enumerate(blocks, start=1):
        if block.kind == "heading":
            flush_buffer()
            current_heading = block.text
            heading_path = block.heading or block.text
            current_section = block.section or "body"
            continue

        if block.heading:
            heading_path = block.heading
        if block.page_no is not None:
            current_page = block.page_no
        if block.section and current_section and block.section != current_section:
            flush_buffer()
        if block.section:
            current_section = block.section

        if block.section == "frontmatter":
            continue

        for piece in split_block_text(block, max_chars):
            candidate = normalize_whitespace("\n\n".join(buffer + [piece]))
            if buffer and len(candidate) > max_chars:
                flush_buffer()
            buffer.append(piece)
            if block_index not in buffer_block_indexes:
                buffer_block_indexes.append(block_index)
            buffer_score = min(buffer_score, chunk_weight(block))
            candidate = normalize_whitespace("\n\n".join(buffer))
            if len(candidate) >= max_chars:
                flush_buffer()
                if emit is not None:
                    emit(
                        ParserStreamMessage(
                            request_id="",
                            kind="event",
                            event="progress",
                            message=f"已切分 {order - 1} 个切片",
                            stage="chunk",
                            percent=min(95, 60 + min(order, 30)),
                            current=path.name,
                            total=len(blocks),
                            processed=block_index,
                            parser_source="python",
                        ).to_dict()
                    )

    flush_buffer()

    if not chunks and blocks:
        joined = normalize_whitespace("\n\n".join(block.text for block in blocks))
        if joined:
            chunks.append(
                ParsedChunk(
                    heading=heading_path or current_heading or path.stem,
                    page_no=None,
                    text=joined,
                    order=1,
                    score=1.0,
                    block_indexes=list(range(1, len(blocks) + 1)) if blocks else None,
                )
            )

    return chunks


def chunk_weight(block: Block) -> float:
    if block.kind == "table":
        return 0.55
    if block.kind == "image":
        return 0.7
    if block.kind in {"quote", "list", "blockquote", "li"}:
        return 0.9
    return 1.0


def looks_like_docx_cover_chunk(text: str) -> bool:
    normalized = normalize_whitespace(text)
    if not normalized:
        return False

    cover_keywords = [
        "文档编号",
        "文档版本",
        "副标题",
        "编制",
        "校对",
        "审核",
        "批准",
        "年月日",
    ]
    if any(keyword in normalized for keyword in cover_keywords):
        return True

    if " | " in normalized and len(normalized) < 260:
        if sum(1 for keyword in ["编制", "校对", "审核", "批准"] if keyword in normalized) >= 2:
            return True

    return False


def split_text(text: str, max_chars: int) -> Iterable[str]:
    cleaned = normalize_whitespace(text)
    if not cleaned:
        return []
    if len(cleaned) <= max_chars:
        return [cleaned]

    pieces: List[str] = []
    start = 0
    separators = [". ", "。", "\n", "; ", "；", ", ", "，", " "]

    while start < len(cleaned):
        end = min(start + max_chars, len(cleaned))
        split_at = -1
        for separator in separators:
            split_at = cleaned.rfind(separator, start, end)
            if split_at > start:
                split_at += len(separator)
                break
        if split_at <= start or split_at > end:
            split_at = end
        piece = cleaned[start:split_at].strip()
        if piece:
            pieces.append(piece)
        start = split_at

    return pieces


def split_block_text(block: Block, max_chars: int) -> Iterable[str]:
    text = normalize_whitespace(block.text)
    if not text:
        return []

    if block.kind in {"heading", "code", "image"}:
        return [text]

    if block.kind in {"table", "list", "quote", "li", "blockquote", "paragraph"}:
        return split_text(text, max_chars)

    return split_text(text, max_chars)


def merge_short_blocks(blocks: Sequence[Block], min_chars: int = 120) -> List[Block]:
    merged: List[Block] = []
    pending: Optional[Block] = None

    for block in blocks:
        if block.kind == "heading":
            if pending is not None:
                merged.append(pending)
                pending = None
            merged.append(block)
            continue

        text = normalize_whitespace(block.text)
        if not text:
            continue

        if pending is None:
            pending = Block(
                kind=block.kind,
                text=text,
                heading=block.heading,
                page_no=block.page_no,
                section=block.section,
                level=block.level,
                markdown=block.markdown,
                html=block.html,
                asset_path=block.asset_path,
                alt_text=block.alt_text,
                caption=block.caption,
                ocr_text=block.ocr_text,
            )
            continue

        same_context = (
            pending.heading == block.heading
            and pending.page_no == block.page_no
            and pending.section == block.section
            and pending.kind == block.kind
        )
        if same_context and len(pending.text) + len(text) < min_chars:
            pending = Block(
                kind=pending.kind,
                text=normalize_whitespace(f"{pending.text}\n\n{text}"),
                heading=pending.heading,
                page_no=pending.page_no,
                section=pending.section,
                level=pending.level,
                markdown=pending.markdown,
                html=pending.html,
                asset_path=pending.asset_path,
                alt_text=pending.alt_text,
                caption=pending.caption,
                ocr_text=pending.ocr_text,
            )
        else:
            merged.append(pending)
            pending = Block(
                kind=block.kind,
                text=text,
                heading=block.heading,
                page_no=block.page_no,
                section=block.section,
                level=block.level,
                markdown=block.markdown,
                html=block.html,
                asset_path=block.asset_path,
                alt_text=block.alt_text,
                caption=block.caption,
                ocr_text=block.ocr_text,
            )

    if pending is not None:
        merged.append(pending)

    return merged


def normalize_blocks(blocks: Sequence[Block]) -> List[Block]:
    normalized: List[Block] = []
    for block in blocks:
        text = normalize_whitespace(block.text)
        if not text:
            continue
        text = strip_noise_paragraph(text)
        if not text:
            continue
        normalized.append(
            Block(
                kind=block.kind,
                text=text,
                heading=block.heading,
                page_no=block.page_no,
                section=block.section,
                level=block.level,
                markdown=block.markdown,
                html=block.html,
                asset_path=block.asset_path,
                alt_text=block.alt_text,
                caption=block.caption,
                ocr_text=block.ocr_text,
            )
        )
    return normalized


def split_paragraphs(text: str) -> List[str]:
    paragraphs: List[str] = []
    current: List[str] = []

    for line in text.replace("\r\n", "\n").split("\n"):
        if not line.strip():
            if current:
                paragraphs.append(normalize_whitespace("\n".join(current)))
                current = []
            continue
        if is_markdown_heading(line.strip()):
            if current:
                paragraphs.append(normalize_whitespace("\n".join(current)))
                current = []
            paragraphs.append(normalize_whitespace(line.strip()))
            continue
        current.append(line)

    if current:
        paragraphs.append(normalize_whitespace("\n".join(current)))

    return [paragraph for paragraph in paragraphs if paragraph]


def extract_docx_text_from_paragraph(paragraph: ElementTree.Element, ns: dict[str, str]) -> str:
    runs: List[str] = []
    for node in paragraph.findall(".//w:t", ns):
        runs.append(node.text or "")
    return clean_docx_text("".join(runs))


def extract_docx_table_rows(table: ElementTree.Element, ns: dict[str, str]) -> List[List[str]]:
    rows: List[List[str]] = []
    for row in table.findall("./w:tr", ns):
        cells: List[str] = []
        for cell in row.findall("./w:tc", ns):
            text = " ".join(
                normalize_whitespace("".join(node.text or "" for node in paragraph.findall(".//w:t", ns)))
                for paragraph in cell.findall("./w:p", ns)
            )
            text = normalize_whitespace(text)
            text = clean_docx_text(text)
            if text:
                cells.append(text)
        row_text = normalize_whitespace(" | ".join(cells))
        if row_text and not looks_like_docx_xml_noise(row_text):
            rows.append(cells)
    return rows


def table_rows_to_formats(rows: Sequence[Sequence[str]]) -> Tuple[str, str, str]:
    normalized_rows = [
        [normalize_whitespace(cell) for cell in row if normalize_whitespace(cell)]
        for row in rows
        if any(normalize_whitespace(cell) for cell in row)
    ]
    normalized_rows = [row for row in normalized_rows if row]
    if not normalized_rows:
        return "", "", ""

    max_cols = max(len(row) for row in normalized_rows)
    padded_rows = [row + [""] * (max_cols - len(row)) for row in normalized_rows]

    if len(padded_rows) == 1:
        header = [f"列 {index}" for index in range(1, max_cols + 1)]
        body_rows = padded_rows
    else:
        header = padded_rows[0]
        body_rows = padded_rows[1:]

    def markdown_row(cells: Sequence[str]) -> str:
        return "| " + " | ".join(cells) + " |"

    def html_row(cells: Sequence[str], tag: str) -> str:
        return "<tr>" + "".join(f"<{tag}>{html.escape(cell)}</{tag}>" for cell in cells) + "</tr>"

    markdown_lines = [markdown_row(header), markdown_row(["---"] * max_cols)]
    for row in body_rows:
        markdown_lines.append(markdown_row(row))

    html_parts = ["<table>", "<thead>", html_row(header, "th"), "</thead>", "<tbody>"]
    for row in body_rows:
        html_parts.append(html_row(row, "td"))
    html_parts.extend(["</tbody>", "</table>"])

    preview_text = normalize_whitespace(" / ".join(" ".join(cell for cell in row if cell) for row in padded_rows))
    return preview_text, "\n".join(markdown_lines), "".join(html_parts)


def parse_fence_line(line: str) -> Optional[str]:
    if line.startswith("```") or line.startswith("~~~"):
        return line[:3]
    return None


def parse_markdown_heading(line: str) -> Optional[str]:
    match = re.match(r"^(#{1,6})\s+(.*)$", line)
    if not match:
        return None
    return normalize_whitespace(match.group(2))


def is_markdown_heading(line: str) -> bool:
    return parse_markdown_heading(line) is not None


def is_markdown_list_item(line: str) -> bool:
    return bool(re.match(r"^(\s*[-*+]\s+|\s*\d+[.)]\s+)", line))


def is_blockquote(line: str) -> bool:
    return line.lstrip().startswith(">")


def is_markdown_table_row(line: str) -> bool:
    return "|" in line and not line.startswith("```")


def strip_markdown_marker(line: str) -> str:
    stripped = line.lstrip()
    stripped = re.sub(r"^[-*+]\s+", "", stripped)
    stripped = re.sub(r"^\d+[.)]\s+", "", stripped)
    stripped = re.sub(r"^>\s?", "", stripped)
    return normalize_whitespace(stripped)


def infer_title_from_blocks(blocks: Sequence[Block]) -> Optional[str]:
    for block in blocks:
        if block.kind == "heading" and block.text:
            return block.text
        if block.text:
            return detect_title_like_line(block.text)
    return None


def detect_title_like_line(text: str) -> Optional[str]:
    first_line = normalize_whitespace(text).split(" ", 1)[0]
    if not first_line:
        return None
    return first_line[:80]


def strip_noise_paragraph(text: str) -> str:
    cleaned = normalize_whitespace(text)
    if not cleaned:
        return ""
    if len(cleaned) <= 1:
        return ""
    if looks_like_docx_xml_noise(cleaned):
        return ""
    if is_noise_line(cleaned):
        return ""
    return cleaned


def is_noise_line(text: str) -> bool:
    lowered = text.lower()
    if lowered in {"copyright", "all rights reserved"}:
        return True
    if re.fullmatch(r"page\s+\d+(\s+of\s+\d+)?", lowered):
        return True
    if re.fullmatch(r"\d+", text):
        return True
    if len(text) <= 3 and not re.search(r"[a-zA-Z\u4e00-\u9fff]", text):
        return True
    return False


def clean_docx_text(text: str) -> str:
    cleaned = normalize_whitespace(text)
    if not cleaned:
        return ""
    cleaned = html.unescape(cleaned)
    cleaned = re.sub(r"\s+", " ", cleaned).strip()
    if looks_like_docx_xml_noise(cleaned):
        return ""
    return cleaned


def looks_like_docx_xml_noise(text: str) -> bool:
    lowered = text.lower()
    if "<w:" in lowered or "</w:" in lowered or "xmlns:" in lowered:
        return True
    if lowered.count("<") >= 2 and lowered.count(">") >= 2 and "w:" in lowered:
        return True
    if lowered.startswith("<") and ">" in lowered and ("w:" in lowered or "xml" in lowered):
        return True
    return False


def normalize_extension(ext: str) -> str:
    return "md" if ext == "markdown" else ext


def normalize_whitespace(text: str) -> str:
    return re.sub(r"\s+", " ", text).strip()


def strip_ns(tag: str) -> str:
    return tag.rsplit("}", 1)[-1] if "}" in tag else tag


def parser_error(code: str, message: str, details: Optional[str] = None) -> Exception:
    return ParserException(ParserError(code=normalize_error_code(code), message=message, details=details))


def normalize_error_code(code: str) -> str:
    normalized = code.strip().lower().replace("-", "_")
    legacy_map = {
        "invalidrequest": "invalid_request",
        "filenotfound": "file_not_found",
        "parsefailed": "parse_failed",
        "internalerror": "internal_error",
        "unsupportedfiletype": "unsupported_file_type",
    }
    return legacy_map.get(normalized.replace("_", ""), normalized)


class ParserException(Exception):
    def __init__(self, error: ParserError) -> None:
        super().__init__(error.message)
        self.error = error
