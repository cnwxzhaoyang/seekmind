from __future__ import annotations

import html.parser
import re
import zipfile
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable, List, Optional, Sequence, Tuple
from xml.etree import ElementTree

from .models import ParsedChunk, ParsedDocument, ParserError, ParserOptions

SUPPORTED_EXTENSIONS = {"txt", "md", "markdown", "html", "htm", "docx"}


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

    def handle_starttag(self, tag: str, attrs):  # type: ignore[override]
        self._stack.append(tag)
        if tag == "title":
            self._flush()
            return
        if tag in self.BLOCK_TAGS:
            self._flush()
            if tag in {"td", "th"}:
                self._current_row.append("")

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
        if text and self.title is None:
            self.title = text

    def _flush_row(self) -> None:
        if self._current_row:
            row_text = normalize_whitespace(" | ".join(cell for cell in self._current_row if cell))
            if row_text:
                self.blocks.append(Block(kind="table", text=row_text, heading=self._current_heading_path()))
        self._current_row.clear()

    def _flush(self) -> None:
        text = normalize_whitespace("".join(self._buffer))
        self._buffer.clear()
        if not text:
            return
        current_tag = self._stack[-1] if self._stack else None
        if current_tag and current_tag.startswith("h") and len(current_tag) == 2 and current_tag[1].isdigit():
            level = int(current_tag[1])
            self._push_heading(level, text)
            self.blocks.append(Block(kind="heading", text=text))
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
        raise parser_error("FILE_NOT_FOUND", f"file not found: {path}")
    if not path.is_file():
        raise parser_error("INVALID_REQUEST", f"not a file: {path}")

    ext = normalize_extension(path.suffix.lower().lstrip("."))
    if ext not in SUPPORTED_EXTENSIONS:
        raise parser_error("UNSUPPORTED_FILE_TYPE", f"unsupported file type: {ext}")

    if ext in {"txt", "md", "markdown"}:
        title, blocks = parse_text_like(path, ext)
    elif ext in {"html", "htm"}:
        title, blocks = parse_html(path)
    elif ext == "docx":
        title, blocks = parse_docx(path)
    else:
        raise parser_error("UNSUPPORTED_FILE_TYPE", f"unsupported file type: {ext}")

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
            blocks.append(Block(kind="paragraph", text=paragraph, heading=heading_path()))
            if title is None:
                title = heading_path() or detect_title_like_line(paragraph) or fallback_title

    def flush_table() -> None:
        nonlocal table_buffer, title
        table = normalize_whitespace(" | ".join(line.strip(" |") for line in table_buffer))
        table_buffer = []
        if table:
            blocks.append(Block(kind="table", text=table, heading=heading_path()))
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
            blocks.append(Block(kind="heading", text=heading))
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
            blocks.append(Block(kind="paragraph", text=strip_markdown_marker(stripped), heading=heading_path()))
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
    return title, extractor.blocks


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


def build_chunks(blocks: Sequence[Block], path: Path, options: ParserOptions) -> List[ParsedChunk]:
    chunks: List[ParsedChunk] = []
    max_chars = max(int(options.max_chunk_chars), 120)
    current_heading: Optional[str] = None
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
                heading=current_heading or path.stem,
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
            continue

        if block.heading:
            current_heading = block.heading
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
                    heading=current_heading or path.stem,
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


def normalize_extension(ext: str) -> str:
    return "md" if ext == "markdown" else ext


def normalize_whitespace(text: str) -> str:
    return re.sub(r"\s+", " ", text).strip()


def strip_ns(tag: str) -> str:
    return tag.rsplit("}", 1)[-1] if "}" in tag else tag


def parser_error(code: str, message: str, details: Optional[str] = None) -> Exception:
    return ParserException(ParserError(code=code, message=message, details=details))


class ParserException(Exception):
    def __init__(self, error: ParserError) -> None:
        super().__init__(error.message)
        self.error = error
