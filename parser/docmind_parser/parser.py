from __future__ import annotations

import html.parser
import re
import subprocess
import zipfile
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable, List, Optional, Sequence, Tuple
from xml.etree import ElementTree

from .models import ParsedChunk, ParsedDocument, ParserError, ParserOptions

SUPPORTED_EXTENSIONS = {"txt", "md", "markdown", "html", "htm", "docx", "pdf"}


@dataclass
class Block:
    kind: str
    text: str
    heading: Optional[str] = None
    page_no: Optional[int] = None


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

    def __init__(self) -> None:
        super().__init__()
        self.blocks: List[Block] = []
        self.title: Optional[str] = None
        self._stack: List[str] = []
        self._buffer: List[str] = []
        self._heading_stack: List[tuple[int, str]] = []
        self._current_row: List[str] = []
        self._row_in_progress = False

    def handle_starttag(self, tag: str, attrs):  # type: ignore[override]
        self._stack.append(tag)
        if tag == "title":
            self._flush()
            return
        if tag in self.BLOCK_TAGS:
            self._flush()
            if tag in {"td", "th"}:
                self._current_row.append("")
                self._row_in_progress = True

    def handle_endtag(self, tag: str):  # type: ignore[override]
        if tag == "title":
            self._flush_title()
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

    def _flush_title(self) -> None:
        text = normalize_whitespace("".join(self._buffer))
        self._buffer.clear()
        text = strip_noise_paragraph(text)
        if text and self.title is None:
            self.title = text

    def _flush_row(self) -> None:
        if self._current_row:
            row_text = normalize_whitespace(" | ".join(cell for cell in self._current_row if cell))
            if row_text:
                self.blocks.append(Block(kind="table", text=row_text, heading=self._current_heading_path()))
        self._current_row.clear()
        self._row_in_progress = False

    def _flush(self) -> None:
        text = normalize_whitespace("".join(self._buffer))
        self._buffer.clear()
        if not text:
            return
        current_tag = self._stack[-1] if self._stack else None
        if current_tag and current_tag.startswith("h") and len(current_tag) == 2 and current_tag[1].isdigit():
            level = int(current_tag[1])
            self._push_heading(level, text)
            self.blocks.append(Block(kind="heading", text=text, heading=self._current_heading_path()))
            return
        if current_tag in {"li", "p", "pre", "blockquote", "td", "th"}:
            self.blocks.append(Block(kind=current_tag or "text", text=text, heading=self._current_heading_path()))
            return
        self.blocks.append(Block(kind="text", text=text, heading=self._current_heading_path()))

    def _push_heading(self, level: int, text: str) -> None:
        while self._heading_stack and self._heading_stack[-1][0] >= level:
            self._heading_stack.pop()
        self._heading_stack.append((level, text))

    def _current_heading_path(self) -> Optional[str]:
        if not self._heading_stack:
            return None
        return " > ".join(heading for _, heading in self._heading_stack)


def parse_document(path: Path, options: ParserOptions) -> ParsedDocument:
    if not path.exists():
        raise parser_error("file_not_found", f"file not found: {path}")
    if not path.is_file():
        raise parser_error("invalid_request", f"not a file: {path}")

    ext = normalize_extension(path.suffix.lower().lstrip("."))
    if ext not in SUPPORTED_EXTENSIONS:
        raise parser_error("unsupported_file_type", f"unsupported file type: {ext}")

    if ext in {"txt", "md", "markdown"}:
        title, blocks = parse_text_like(path, ext)
    elif ext in {"html", "htm"}:
        title, blocks = parse_html(path)
    elif ext == "docx":
        title, blocks = parse_docx(path)
    elif ext == "pdf":
        title, blocks = parse_pdf(path)
    else:
        raise parser_error("unsupported_file_type", f"unsupported file type: {ext}")

    blocks = merge_short_blocks(normalize_blocks(blocks))
    content = "\n\n".join(block.text for block in blocks if block.text.strip())
    chunks = build_chunks(blocks, path, options)
    if options.max_chunks is not None:
        chunks = chunks[: max(int(options.max_chunks), 0)]

    return ParsedDocument(
        title=title or path.stem,
        file_type=ext,
        content=content,
        chunks=chunks if options.include_chunks else [],
    )


def parse_text_like(path: Path, ext: str) -> Tuple[Optional[str], List[Block]]:
    text = path.read_text(encoding="utf-8", errors="ignore").replace("\r\n", "\n")
    if ext == "txt":
        return parse_plain_text(text, path.stem)
    return parse_markdown(text, path.stem)


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


def parse_markdown(text: str, fallback_title: str) -> Tuple[Optional[str], List[Block]]:
    blocks: List[Block] = []
    title: Optional[str] = None
    heading_stack: List[tuple[int, str]] = []
    buffer: List[str] = []
    in_code = False
    code_fence = ""
    table_buffer: List[str] = []
    current_heading: Optional[str] = None

    def heading_path() -> Optional[str]:
        if not heading_stack:
            return None
        return " > ".join(heading for _, heading in heading_stack)

    def flush_buffer() -> None:
        nonlocal buffer, title
        paragraph = normalize_whitespace("\n".join(buffer))
        buffer = []
        if paragraph:
            cleaned = strip_noise_paragraph(paragraph)
            if cleaned:
                blocks.append(Block(kind="paragraph", text=cleaned, heading=heading_path()))
            if title is None:
                title = heading_path() or detect_title_like_line(paragraph) or fallback_title

    def flush_table() -> None:
        nonlocal table_buffer, title
        table = normalize_whitespace(" | ".join(line.strip(" |") for line in table_buffer))
        table_buffer = []
        if table:
            cleaned = strip_noise_paragraph(table)
            if cleaned:
                blocks.append(Block(kind="table", text=cleaned, heading=heading_path()))
            if title is None:
                title = heading_path() or fallback_title

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
                buffer.append(stripped)
            else:
                buffer.append(stripped)
                if fence == code_fence:
                    in_code = False
                    flush_buffer()
                    code_fence = ""
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
            current_heading = heading_path()
            blocks.append(Block(kind="heading", text=heading, heading=heading_path()))
            if title is None:
                title = current_heading or heading
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

        if is_markdown_list_item(stripped) or is_blockquote(stripped):
            flush_buffer()
            cleaned = strip_noise_paragraph(strip_markdown_marker(stripped))
            if cleaned:
                blocks.append(Block(kind="paragraph", text=cleaned, heading=heading_path()))
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
    extractor = HtmlBlockExtractor()
    extractor.feed(text)
    extractor.close()
    title = extractor.title or infer_title_from_blocks(extractor.blocks) or path.stem
    return title, normalize_blocks(extractor.blocks)


def parse_docx(path: Path) -> Tuple[Optional[str], List[Block]]:
    with zipfile.ZipFile(path) as archive:
        document_xml = archive.read("word/document.xml")

    ns = {"w": "http://schemas.openxmlformats.org/wordprocessingml/2006/main"}
    root = ElementTree.fromstring(document_xml)
    blocks: List[Block] = []
    title: Optional[str] = None
    heading_stack: List[tuple[int, str]] = []

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
                while heading_stack and heading_stack[-1][0] >= level:
                    heading_stack.pop()
                heading_stack.append((level, text))
                blocks.append(Block(kind="heading", text=text))
                if title is None:
                    title = heading_path() or text
            else:
                blocks.append(Block(kind="paragraph", text=text, heading=heading_path()))
                if title is None:
                    title = heading_path() or detect_title_like_line(text) or path.stem
        elif tag == "tbl":
            rows = extract_docx_table(element, ns)
            for row in rows:
                if row:
                    blocks.append(Block(kind="table", text=row, heading=heading_path()))
                    if title is None:
                        title = heading_path() or path.stem

    return title or path.stem, blocks


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


def build_chunks(blocks: Sequence[Block], path: Path, options: ParserOptions) -> List[ParsedChunk]:
    chunks: List[ParsedChunk] = []
    max_chars = max(int(options.max_chunk_chars), 120)
    current_heading: Optional[str] = None
    heading_path: Optional[str] = None
    current_page: Optional[int] = None
    buffer: List[str] = []
    order = 1

    def flush_buffer() -> None:
        nonlocal buffer, order
        text = normalize_whitespace("\n\n".join(buffer))
        buffer = []
        if not text:
            return
        chunks.append(
            ParsedChunk(
                heading=heading_path or current_heading or path.stem,
                page_no=current_page,
                text=text,
                order=order,
            )
        )
        order += 1

    for block in blocks:
        if block.kind == "heading":
            flush_buffer()
            current_heading = block.text
            heading_path = block.heading or block.text
            continue

        if block.heading:
            heading_path = block.heading
        if block.page_no is not None:
            current_page = block.page_no

        for piece in split_block_text(block, max_chars):
            candidate = normalize_whitespace("\n\n".join(buffer + [piece]))
            if buffer and len(candidate) > max_chars:
                flush_buffer()
            buffer.append(piece)
            candidate = normalize_whitespace("\n\n".join(buffer))
            if len(candidate) >= max_chars:
                flush_buffer()

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
                )
            )

    return chunks


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

    if block.kind in {"heading"}:
        return [text]

    if block.kind in {"table", "li", "blockquote", "paragraph"}:
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
            pending = Block(kind=block.kind, text=text, heading=block.heading, page_no=block.page_no)
            continue

        same_context = pending.heading == block.heading and pending.page_no == block.page_no
        if same_context and len(pending.text) + len(text) < min_chars:
            pending = Block(
                kind=pending.kind,
                text=normalize_whitespace(f"{pending.text}\n\n{text}"),
                heading=pending.heading,
                page_no=pending.page_no,
            )
        else:
            merged.append(pending)
            pending = Block(kind=block.kind, text=text, heading=block.heading, page_no=block.page_no)

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
    return "".join(runs)


def extract_docx_table(table: ElementTree.Element, ns: dict[str, str]) -> List[str]:
    rows: List[str] = []
    for row in table.findall("./w:tr", ns):
        cells: List[str] = []
        for cell in row.findall("./w:tc", ns):
            text = " ".join(
                normalize_whitespace("".join(node.text or "" for node in paragraph.findall(".//w:t", ns)))
                for paragraph in cell.findall("./w:p", ns)
            )
            text = normalize_whitespace(text)
            if text:
                cells.append(text)
        row_text = normalize_whitespace(" | ".join(cells))
        if row_text:
            rows.append(row_text)
    return rows


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
