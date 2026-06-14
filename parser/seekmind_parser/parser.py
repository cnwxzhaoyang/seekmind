"""
@author MorningSun
@CreatedDate 2026/06/05
@Description SeekMind 文档解析、OCR 队列与格式抽取实现。
"""

from __future__ import annotations

import hashlib
import html
import html.parser
import os
import re
import subprocess
import tempfile
import shutil
import sys
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
from .pdf_ocr import PdfOcrTask, build_pdf_ocr_task

SUPPORTED_EXTENSIONS = {"txt", "md", "markdown", "html", "htm", "doc", "docx", "ppt", "pptx", "pdf"}
DEFAULT_EMBEDDING_MODEL = "BAAI/bge-small-zh-v1.5"
EMBEDDING_DIMENSION_HINTS = {
    "BAAI/bge-small-zh-v1.5": 512,
    "BAAI/bge-small-en-v1.5": 384,
    "sentence-transformers/all-MiniLM-L6-v2": 384,
    "jinaai/jina-embeddings-v2-base-zh": 768,
}


def env_value(*names: str) -> Optional[str]:
    for name in names:
        value = os.environ.get(name)
        if value and value.strip():
            return value.strip()
    return None


DOCX_NAMESPACES = {
    "w": "http://schemas.openxmlformats.org/wordprocessingml/2006/main",
    "a": "http://schemas.openxmlformats.org/drawingml/2006/main",
    "r": "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
    "pic": "http://schemas.openxmlformats.org/drawingml/2006/picture",
}

PPTX_NAMESPACES = {
    "a": "http://schemas.openxmlformats.org/drawingml/2006/main",
    "p": "http://schemas.openxmlformats.org/presentationml/2006/main",
    "r": "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
}


@dataclass
class Block:
    kind: str
    text: str
    heading: Optional[str] = None
    page_no: Optional[int] = None
    section: Optional[str] = None
    level: Optional[int] = None
    language: Optional[str] = None
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


def docx_media_cache_dir(document_path: Path) -> Path:
    digest = hashlib.sha1(str(document_path.resolve()).encode("utf-8")).hexdigest()[:16]
    cache_dir = Path(tempfile.gettempdir()) / "seekmind-docx-media" / digest
    cache_dir.mkdir(parents=True, exist_ok=True)
    return cache_dir


def docx_media_output_path(document_path: Path, source_name: str, fallback_ext: str = "") -> Path:
    base_name = Path(source_name).name or "image"
    suffix = Path(base_name).suffix or fallback_ext
    stem = Path(base_name).stem or "image"
    digest = hashlib.sha1(source_name.encode("utf-8")).hexdigest()[:10]
    safe_name = f"{stem}-{digest}{suffix}"
    return docx_media_cache_dir(document_path) / safe_name


def pptx_media_cache_dir(document_path: Path) -> Path:
    digest = hashlib.sha1(str(document_path.resolve()).encode("utf-8")).hexdigest()[:16]
    cache_dir = Path(tempfile.gettempdir()) / "seekmind-pptx-media" / digest
    cache_dir.mkdir(parents=True, exist_ok=True)
    return cache_dir


def pptx_media_output_path(document_path: Path, source_name: str, fallback_ext: str = "") -> Path:
    base_name = Path(source_name).name or "image"
    suffix = Path(base_name).suffix or fallback_ext
    stem = Path(base_name).stem or "image"
    digest = hashlib.sha1(source_name.encode("utf-8")).hexdigest()[:10]
    safe_name = f"{stem}-{digest}{suffix}"
    return pptx_media_cache_dir(document_path) / safe_name


def store_pptx_media_bytes(document_path: Path, source_name: str, data: bytes) -> str:
    output_path = pptx_media_output_path(document_path, source_name)
    if not output_path.exists():
        output_path.write_bytes(data)
    return str(output_path)


@lru_cache(maxsize=1)
def office_converter_path() -> Optional[str]:
    candidates: list[Optional[str]] = [os.environ.get("SEEKMIND_OFFICE_BIN")]
    if sys.platform.startswith("win"):
        candidates.extend(
            [
                shutil.which("soffice.exe"),
                shutil.which("libreoffice.exe"),
                os.environ.get("PROGRAMFILES") and str(Path(os.environ["PROGRAMFILES"]) / "LibreOffice" / "program" / "soffice.exe"),
                os.environ.get("PROGRAMFILES(X86)") and str(
                    Path(os.environ["PROGRAMFILES(X86)"]) / "LibreOffice" / "program" / "soffice.exe"
                ),
            ]
        )
    elif sys.platform == "darwin":
        candidates.extend(
            [
                shutil.which("soffice"),
                shutil.which("libreoffice"),
                "/Applications/LibreOffice.app/Contents/MacOS/soffice",
                "/Applications/LibreOffice.app/Contents/MacOS/libreoffice",
            ]
        )
    else:
        candidates.extend([shutil.which("soffice"), shutil.which("libreoffice")])

    for candidate in candidates:
        if candidate and office_converter_works(candidate):
            return candidate
    return None


@lru_cache(maxsize=16)
def office_converter_works(candidate: str) -> bool:
    if not candidate.strip():
        return False
    try:
        result = subprocess.run(
            [candidate, "--version"],
            check=False,
            capture_output=True,
            text=True,
            timeout=10,
        )
    except Exception:  # noqa: BLE001
        return False
    return result.returncode == 0


def office_conversion_cache_dir(document_path: Path) -> Path:
    try:
        metadata = document_path.stat()
        cache_key = f"{document_path.resolve()}::{metadata.st_mtime_ns}::{metadata.st_size}"
    except Exception:  # noqa: BLE001
        cache_key = str(document_path.resolve())
    digest = hashlib.sha1(cache_key.encode("utf-8")).hexdigest()[:16]
    cache_dir = Path(tempfile.gettempdir()) / "seekmind-office-convert" / digest
    cache_dir.mkdir(parents=True, exist_ok=True)
    return cache_dir


def office_conversion_output_path(document_path: Path, target_ext: str) -> Path:
    stem = document_path.stem or "document"
    suffix = f".{target_ext.lstrip('.').lower()}"
    return office_conversion_cache_dir(document_path) / f"{stem}{suffix}"


def convert_document_with_office(document_path: Path, target_ext: str) -> Optional[Path]:
    converter = office_converter_path()
    if not converter:
        return None

    target_ext = target_ext.lstrip(".").lower()
    output_path = office_conversion_output_path(document_path, target_ext)
    if output_path.exists() and output_path.stat().st_size > 0:
        return output_path

    try:
        result = subprocess.run(
            [
                converter,
                "--headless",
                "--nologo",
                "--nofirststartwizard",
                "--convert-to",
                target_ext,
                "--outdir",
                str(output_path.parent),
                str(document_path),
            ],
            check=False,
            capture_output=True,
            text=True,
            timeout=60,
        )
    except FileNotFoundError:
        return None
    except subprocess.TimeoutExpired:
        return None
    except Exception:  # noqa: BLE001
        return None

    if result.returncode != 0:
        return None

    if output_path.exists() and output_path.stat().st_size > 0:
        return output_path

    candidates = sorted(
        (path for path in output_path.parent.glob(f"*.{target_ext}") if path.is_file() and path.stat().st_size > 0),
        key=lambda item: item.stat().st_mtime,
        reverse=True,
    )
    return candidates[0] if candidates else None


def normalize_docx_media_target(target: str) -> str:
    cleaned = normalize_whitespace(target).replace("\\", "/")
    cleaned = cleaned.lstrip("/")
    while cleaned.startswith("../"):
        cleaned = cleaned[3:]
    if cleaned.startswith("word/"):
        return cleaned
    if cleaned:
        return f"word/{cleaned}"
    return ""


def store_docx_media_bytes(document_path: Path, source_name: str, data: bytes) -> str:
    output_path = docx_media_output_path(document_path, source_name)
    if not output_path.exists():
        output_path.write_bytes(data)
    return str(output_path)


def load_docx_relationship_targets(archive: zipfile.ZipFile) -> dict[str, str]:
    try:
        rel_xml = archive.read("word/_rels/document.xml.rels")
    except KeyError:
        return {}

    try:
        root = ElementTree.fromstring(rel_xml)
    except Exception:  # noqa: BLE001
        return {}

    targets: dict[str, str] = {}
    for rel in root:
        if strip_ns(rel.tag) != "Relationship":
            continue
        rel_id = rel.attrib.get("Id", "").strip()
        target = rel.attrib.get("Target", "").strip()
        if not rel_id or not target:
            continue
        if rel.attrib.get("TargetMode", "").lower() == "external":
            continue
        targets[rel_id] = target
    return targets


def docx_vector_image_note(source_name: str, content_type: Optional[str] = None) -> Optional[str]:
    lowered_name = source_name.lower()
    lowered_type = (content_type or "").lower()
    if any(token in lowered_name for token in {".x-emf", ".emf", ".wmf"}):
        return "Office 矢量图暂不支持预览"
    if any(token in lowered_type for token in {"emf", "wmf"}):
        return "Office 矢量图暂不支持预览"
    return None



def is_docx_vector_image_source(source_name: str, content_type: Optional[str] = None) -> bool:
    lowered_name = source_name.lower()
    lowered_type = (content_type or "").lower()
    return any(token in lowered_name for token in {".x-emf", ".emf", ".wmf"}) or any(
        token in lowered_type for token in {"emf", "wmf"}
    )



def docx_vector_image_converter_path() -> Optional[str]:
    return office_converter_path()



def docx_vector_image_output_dir(document_path: Path, source_path: Path) -> Path:
    digest = hashlib.sha256(f"{document_path}::{source_path}".encode("utf-8")).hexdigest()[:16]
    return docx_media_cache_dir(document_path) / "soffice" / digest



def docx_vector_image_input_suffix(source_path: Path) -> str:
    lowered = source_path.suffix.lower()
    if lowered in {".emf", ".x-emf"}:
        return ".emf"
    if lowered in {".wmf", ".x-wmf"}:
        return ".wmf"
    return source_path.suffix or ".emf"



def try_convert_docx_vector_image_to_png(document_path: Path, source_path: Path) -> Optional[Path]:
    converter = docx_vector_image_converter_path()
    if not converter:
        return None

    output_dir = docx_vector_image_output_dir(document_path, source_path)
    output_dir.mkdir(parents=True, exist_ok=True)

    input_suffix = docx_vector_image_input_suffix(source_path)
    working_input = output_dir / f"{source_path.stem}{input_suffix}"
    expected_output = output_dir / f"{working_input.stem}.png"

    if expected_output.exists() and expected_output.stat().st_size > 0:
        return expected_output

    try:
        if source_path.resolve() != working_input.resolve():
            working_input.write_bytes(source_path.read_bytes())
    except Exception:  # noqa: BLE001
        return None

    for existing in output_dir.glob("*.png"):
        try:
            existing.unlink()
        except Exception:  # noqa: BLE001
            pass

    try:
        result = subprocess.run(
            [converter, "--headless", "--nologo", "--nofirststartwizard", "--convert-to", "png", "--outdir", str(output_dir), str(working_input)],
            check=False,
            capture_output=True,
            text=True,
            timeout=60,
        )
    except FileNotFoundError:
        return None
    except subprocess.TimeoutExpired:
        return None
    except Exception:  # noqa: BLE001
        return None

    if result.returncode != 0:
        return None

    if expected_output.exists() and expected_output.stat().st_size > 0:
        return expected_output

    candidates = sorted(
        (path for path in output_dir.glob("*.png") if path.is_file() and path.stat().st_size > 0),
        key=lambda item: item.stat().st_mtime,
        reverse=True,
    )
    return candidates[0] if candidates else None



def resolve_docx_image_asset_path(
    document_path: Path,
    source_name: str,
    data: bytes,
    content_type: Optional[str] = None,
) -> tuple[str, Optional[str]]:
    stored_path = Path(store_docx_media_bytes(document_path, source_name, data))
    if is_docx_vector_image_source(source_name, content_type):
        converted_path = try_convert_docx_vector_image_to_png(document_path, stored_path)
        if converted_path is not None:
            return str(converted_path), None
        return str(stored_path), docx_vector_image_note(source_name, content_type)
    return str(stored_path), None



def build_docx_image_block(
    asset_path: str,
    heading: Optional[str],
    section: Optional[str],
    alt_text: Optional[str] = None,
    caption: Optional[str] = None,
    preview_note: Optional[str] = None,
) -> Block:
    label = normalize_whitespace(alt_text or caption or image_label_from_path(asset_path) or "image")
    note = normalize_whitespace(preview_note or "")
    final_caption = caption or (note if note else None)
    return Block(
        kind="image",
        text=label,
        heading=heading,
        section=section,
        markdown=f"![{label}]({asset_path})",
        html=build_img_html(asset_path, alt_text or label, final_caption or ""),
        asset_path=asset_path or None,
        alt_text=alt_text or None,
        caption=final_caption,
        ocr_text=note or None,
    )


def parse_docx_list_info_from_style(style_name: str) -> tuple[Optional[str], int]:
    lowered = normalize_whitespace(style_name).lower()
    if not lowered:
        return None, 0

    list_kind: Optional[str] = None
    if "list bullet" in lowered or "bullet" in lowered:
        list_kind = "bullet"
    elif "list number" in lowered or "number" in lowered:
        list_kind = "ordered"

    if list_kind is None:
        return None, 0

    level_match = re.search(r"(\d+)$", lowered)
    level = max(int(level_match.group(1)) - 1, 0) if level_match else 0
    return list_kind, level


def parse_docx_list_info_from_element(element: ElementTree.Element, ns: dict[str, str], style_name: str = "") -> tuple[Optional[str], int]:
    style_kind, style_level = parse_docx_list_info_from_style(style_name)
    num_pr = element.find("./w:pPr/w:numPr", ns)
    if num_pr is None:
        return style_kind, style_level

    ilvl_node = num_pr.find("./w:ilvl", ns)
    ilvl = 0
    if ilvl_node is not None:
        try:
            ilvl = int(ilvl_node.attrib.get(f"{{{ns['w']}}}val", "0"))
        except ValueError:
            ilvl = 0

    num_kind = style_kind or "bullet"
    return num_kind, max(ilvl, style_level)


def format_docx_list_markdown(text: str, list_kind: str, level: int) -> str:
    indent = "    " * max(level, 0)
    marker = "1." if list_kind == "ordered" else "-"
    cleaned = normalize_whitespace(text)
    return f"{indent}{marker} {cleaned}" if cleaned else ""


def extract_docx_ordered_inline_blocks_from_element(
    paragraph: ElementTree.Element,
    archive: zipfile.ZipFile,
    document_path: Path,
    rel_targets: dict[str, str],
    heading: Optional[str],
    section: Optional[str],
    list_kind: Optional[str] = None,
    list_level: int = 0,
) -> List[Block]:
    blocks: List[Block] = []
    pending_parts: List[str] = []

    def emit_text_block() -> None:
        nonlocal pending_parts
        text = normalize_whitespace(" ".join(pending_parts))
        pending_parts = []
        if not text:
            return
        if list_kind is not None:
            markdown = format_docx_list_markdown(text, list_kind, list_level)
            blocks.append(
                Block(
                    kind="list",
                    text=text,
                    heading=heading,
                    section=section,
                    level=list_level,
                    markdown=markdown,
                )
            )
        else:
            blocks.append(
                Block(
                    kind="paragraph",
                    text=text,
                    heading=heading,
                    section=section,
                    markdown=text,
                )
            )

    def emit_images_from_node(node: ElementTree.Element) -> None:
        for blip in node.findall(".//a:blip", DOCX_NAMESPACES):
            rel_id = blip.attrib.get(f"{{{DOCX_NAMESPACES['r']}}}embed") or blip.attrib.get(
                f"{{{DOCX_NAMESPACES['r']}}}link"
            )
            if not rel_id:
                continue
            target = rel_targets.get(rel_id, "")
            archive_path = normalize_docx_media_target(target)
            if not archive_path:
                continue
            try:
                data = archive.read(archive_path)
            except KeyError:
                continue
            asset_path, preview_note = resolve_docx_image_asset_path(
                document_path,
                archive_path,
                data,
            )
            blocks.append(
                build_docx_image_block(
                    asset_path,
                    heading,
                    section,
                    preview_note=preview_note,
                )
            )

    def process_container(container: ElementTree.Element) -> None:
        for child in list(container):
            tag = strip_ns(child.tag)
            if tag == "r":
                for inline in list(child):
                    inline_tag = strip_ns(inline.tag)
                    if inline_tag == "t":
                        if inline.text:
                            pending_parts.append(inline.text)
                    elif inline_tag in {"tab"}:
                        pending_parts.append("\t")
                    elif inline_tag in {"br"}:
                        pending_parts.append("\n")
                    elif inline_tag in {"drawing", "pict"}:
                        emit_text_block()
                        emit_images_from_node(inline)
                continue
            if tag == "hyperlink":
                process_container(child)
                continue
            # fall back to deep scan for nested drawing-only containers
            if child.findall(".//a:blip", DOCX_NAMESPACES):
                emit_text_block()
                emit_images_from_node(child)

    process_container(paragraph)
    emit_text_block()
    return blocks


def extract_docx_inline_image_blocks_from_paragraph(
    paragraph: ElementTree.Element,
    archive: zipfile.ZipFile,
    document_path: Path,
    rel_targets: dict[str, str],
    heading: Optional[str],
    section: Optional[str],
) -> List[Block]:
    image_blocks: List[Block] = []
    seen: set[str] = set()
    for blip in paragraph.findall(".//a:blip", DOCX_NAMESPACES):
        rel_id = blip.attrib.get(f"{{{DOCX_NAMESPACES['r']}}}embed") or blip.attrib.get(
            f"{{{DOCX_NAMESPACES['r']}}}link"
        )
        if not rel_id or rel_id in seen:
            continue
        seen.add(rel_id)
        target = rel_targets.get(rel_id, "")
        archive_path = normalize_docx_media_target(target)
        if not archive_path:
            continue
        try:
            data = archive.read(archive_path)
        except KeyError:
            continue
        asset_path, preview_note = resolve_docx_image_asset_path(
            document_path,
            archive_path,
            data,
        )
        image_blocks.append(
            build_docx_image_block(
                asset_path,
                heading,
                section,
                preview_note=preview_note,
            )
        )
    return image_blocks


def extract_docx_inline_image_blocks_from_python_docx_paragraph(
    paragraph: object,
    document_path: Path,
    heading: Optional[str],
    section: Optional[str],
    related_parts: dict[str, object],
) -> List[Block]:
    image_blocks: List[Block] = []
    seen: set[str] = set()
    paragraph_element = getattr(paragraph, "_element", None)
    if paragraph_element is None:
        return image_blocks

    for blip in paragraph_element.findall(".//a:blip", DOCX_NAMESPACES):
        rel_id = blip.attrib.get(f"{{{DOCX_NAMESPACES['r']}}}embed") or blip.attrib.get(
            f"{{{DOCX_NAMESPACES['r']}}}link"
        )
        if not rel_id or rel_id in seen:
            continue
        seen.add(rel_id)
        part = related_parts.get(rel_id)
        blob = getattr(part, "blob", None) if part is not None else None
        if not blob:
            continue
        content_type = getattr(part, "content_type", "") or ""
        suffix = ""
        if "/" in content_type:
            suffix = f".{content_type.rsplit('/', 1)[-1].split('+', 1)[0]}"
        source_name = f"{rel_id}{suffix}"
        asset_path, preview_note = resolve_docx_image_asset_path(
            document_path,
            source_name,
            bytes(blob),
            content_type,
        )
        image_blocks.append(
            build_docx_image_block(
                asset_path,
                heading,
                section,
                preview_note=preview_note,
            )
        )
    return image_blocks


def extract_docx_ordered_inline_blocks_from_python_docx_paragraph(
    paragraph: object,
    document_path: Path,
    heading: Optional[str],
    section: Optional[str],
    related_parts: dict[str, object],
    list_kind: Optional[str] = None,
    list_level: int = 0,
) -> List[Block]:
    blocks: List[Block] = []
    pending_parts: List[str] = []
    paragraph_element = getattr(paragraph, "_element", None)
    if paragraph_element is None:
        return blocks

    def emit_text_block() -> None:
        nonlocal pending_parts
        text = normalize_whitespace(" ".join(pending_parts))
        pending_parts = []
        if not text:
            return
        if list_kind is not None:
            markdown = format_docx_list_markdown(text, list_kind, list_level)
            blocks.append(
                Block(
                    kind="list",
                    text=text,
                    heading=heading,
                    section=section,
                    level=list_level,
                    markdown=markdown,
                )
            )
        else:
            blocks.append(
                Block(
                    kind="paragraph",
                    text=text,
                    heading=heading,
                    section=section,
                    markdown=text,
                )
            )

    def emit_images_from_rel_id(rel_id: str) -> None:
        part = related_parts.get(rel_id)
        blob = getattr(part, "blob", None) if part is not None else None
        if not blob:
            return
        content_type = getattr(part, "content_type", "") or ""
        suffix = ""
        if "/" in content_type:
            suffix = f".{content_type.rsplit('/', 1)[-1].split('+', 1)[0]}"
        source_name = f"{rel_id}{suffix}"
        asset_path, preview_note = resolve_docx_image_asset_path(
            document_path,
            source_name,
            bytes(blob),
            content_type,
        )
        blocks.append(
            build_docx_image_block(
                asset_path,
                heading,
                section,
                preview_note=preview_note,
            )
        )

    def process_container(container: ElementTree.Element) -> None:
        for child in list(container):
            tag = strip_ns(child.tag)
            if tag == "r":
                for inline in list(child):
                    inline_tag = strip_ns(inline.tag)
                    if inline_tag == "t":
                        if inline.text:
                            pending_parts.append(inline.text)
                    elif inline_tag in {"tab"}:
                        pending_parts.append("\t")
                    elif inline_tag in {"br"}:
                        pending_parts.append("\n")
                    elif inline_tag in {"drawing", "pict"}:
                        emit_text_block()
                        for blip in inline.findall(".//a:blip", DOCX_NAMESPACES):
                            rel_id = blip.attrib.get(f"{{{DOCX_NAMESPACES['r']}}}embed") or blip.attrib.get(
                                f"{{{DOCX_NAMESPACES['r']}}}link"
                            )
                            if rel_id:
                                emit_images_from_rel_id(rel_id)
                continue
            if tag == "hyperlink":
                process_container(child)
                continue
            if child.findall(".//a:blip", DOCX_NAMESPACES):
                emit_text_block()
                for blip in child.findall(".//a:blip", DOCX_NAMESPACES):
                    rel_id = blip.attrib.get(f"{{{DOCX_NAMESPACES['r']}}}embed") or blip.attrib.get(
                        f"{{{DOCX_NAMESPACES['r']}}}link"
                    )
                    if rel_id:
                        emit_images_from_rel_id(rel_id)

    process_container(paragraph_element)
    emit_text_block()
    return blocks


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

def parse_html_img_like_line(raw_line: str, base_path: Path, heading: Optional[str]) -> Optional[Block]:
    stripped = raw_line.strip()
    if not (stripped.startswith("<img") and "src=" in stripped):
        return None

    attrs: dict[str, str] = {}
    for key in ("src", "alt", "title"):
        match = re.search(rf"{key}\s*=\s*([\"\'])(.*?)\1", stripped, flags=re.IGNORECASE)
        if match:
            attrs[key] = match.group(2)

    src = normalize_whitespace(attrs.get("src", ""))
    if not src:
        return None

    alt = normalize_whitespace(attrs.get("alt", ""))
    caption = normalize_whitespace(attrs.get("title", ""))
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
    ocr_tasks: List[PdfOcrTask] = []

    if ext in {"txt", "md", "markdown"}:
        title, blocks = parse_text_like(path, ext)
    elif ext in {"html", "htm"}:
        title, blocks = parse_html(path)
    elif ext == "doc":
        title, blocks = parse_doc(path)
    elif ext == "docx":
        title, blocks = parse_docx(path)
    elif ext == "pptx":
        title, blocks = parse_pptx(path)
    elif ext == "ppt":
        title, blocks = parse_ppt(path, emit=emit, request_id=request_id)
    elif ext == "pdf":
        title, blocks, ocr_tasks = parse_pdf(path, emit=emit, request_id=request_id)
    else:
        raise parser_error("unsupported_file_type", f"unsupported file type: {ext}")

    progress("extract", f"已提取 {len(blocks)} 个内容块", 35, path.name, len(blocks), len(blocks))
    blocks = merge_short_blocks(normalize_blocks(blocks))
    progress("normalize", "正在整理内容块", 60, path.name, len(blocks), len(blocks))
    content = "\n\n".join(
        block.text for block in blocks if block.text.strip() and block.section != "frontmatter"
    )
    chunks = build_chunks(blocks, path, options, emit=emit, request_id=request_id)
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
            language=block.language,
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
        ocr_tasks=ocr_tasks or None,
    )


def embedding_status(model_name: Optional[str] = None) -> EmbeddingStatus:
    # 修复：设置页不能把 fastembed 可导入但模型尚未下载/加载的 lazy 状态误判成“embedding 已可用”；
    # 这里改成真实探测，只有模型能实际拉起时才返回 available=True。
    runtime = get_fastembed_runtime(model_name, eager=True)
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


def fastembed_cache_dir() -> Optional[Path]:
    cache_dir = env_value("SEEKMIND_FASTEMBED_CACHE_DIR", "SeekMind_FASTEMBED_CACHE_DIR")
    if not cache_dir:
        return None
    return Path(cache_dir)


def looks_like_broken_fastembed_cache(error: Exception) -> bool:
    message = normalize_embedding_error(error).lower()
    return (
        "no_suchfile" in message
        or "file doesn't exist" in message
        or "model_optimized.onnx" in message
        or "load model" in message and ".onnx" in message
    )


def purge_fastembed_cache(cache_dir: Optional[Path]) -> bool:
    if cache_dir is None or not cache_dir.exists():
        return False

    try:
        # 修复：首次下载超时会留下半拉子的 HF/ONNX 缓存，后续探测会一直命中坏文件。
        # 这里探测到典型坏缓存后主动清空并重试一次，让设置页可以自愈恢复。
        shutil.rmtree(cache_dir)
        cache_dir.mkdir(parents=True, exist_ok=True)
        return True
    except Exception as exc:  # noqa: BLE001
        print(
            f"[SeekMind] failed to purge broken fastembed cache dir={cache_dir}: {exc}",
            file=sys.stderr,
        )
        return False


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

    cache_dir = env_value("SEEKMIND_FASTEMBED_CACHE_DIR", "SeekMind_FASTEMBED_CACHE_DIR")

    def build_runtime() -> FastEmbedRuntime:
        model = TextEmbedding(
            model_name=target_model,
            # 修复：兼容历史的 SeekMind_FASTEMBED_CACHE_DIR，同时优先读取新的 SEEKMIND_FASTEMBED_CACHE_DIR，
            # 避免 warmup 与桌面端启动写入到两个不同模型缓存目录。
            cache_dir=cache_dir,
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

    try:
        return build_runtime()
    except Exception as exc:  # noqa: BLE001
        if looks_like_broken_fastembed_cache(exc) and purge_fastembed_cache(fastembed_cache_dir()):
            print(
                f"[SeekMind] detected broken fastembed cache for model={target_model}, cache reset and retrying",
                file=sys.stderr,
            )
            try:
                return build_runtime()
            except Exception as retry_exc:  # noqa: BLE001
                message = normalize_embedding_error(retry_exc)
                return FastEmbedRuntime(
                    available=False,
                    provider="fastembed",
                    model_name=target_model,
                    model_path="",
                    dimension=0,
                    message=message,
                    model=None,
                )

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
                        blocks.append(Block(kind="code", text=code_text, heading=heading_path(), language=code_lang or None))
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

        html_image = parse_html_img_like_line(stripped, base_path, heading_path())
        if html_image is not None:
            flush_buffer()
            flush_table()
            blocks.append(html_image)
            if title is None:
                title = html_image.heading or detect_title_like_line(html_image.text) or fallback_title
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
        rel_targets = load_docx_relationship_targets(archive)

        ns = {"w": DOCX_NAMESPACES["w"]}
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
                section = "body" if seen_body_heading else "frontmatter"
                image_blocks = extract_docx_inline_image_blocks_from_paragraph(
                    element,
                    archive,
                    path,
                    rel_targets,
                    heading_path(),
                    section,
                )

                if style and style.lower().startswith("heading"):
                    match = re.search(r"heading(\d+)", style.lower())
                    level = int(match.group(1)) if match else 1
                    if title is None and text:
                        title = text
                        blocks.append(
                            Block(
                                kind="heading",
                                text=text,
                                level=level,
                                section="frontmatter",
                                markdown=f"{'#' * level} {text}",
                            )
                        )
                        blocks.extend(image_blocks)
                        continue
                    while heading_stack and heading_stack[-1][0] >= level:
                        heading_stack.pop()
                    heading_stack.append((level, text or heading_path() or path.stem))
                    seen_body_heading = True
                    heading_text = text or heading_path() or path.stem
                    blocks.append(
                        Block(
                            kind="heading",
                            text=heading_text,
                            level=level,
                            section="body",
                            markdown=f"{'#' * level} {heading_text}",
                        )
                    )
                    blocks.extend(image_blocks)
                else:
                    list_kind, list_level = parse_docx_list_info_from_element(element, ns, style or "")
                    if text:
                        ordered_blocks = extract_docx_ordered_inline_blocks_from_element(
                            element,
                            archive,
                            path,
                            rel_targets,
                            heading_path(),
                            section,
                            list_kind=list_kind,
                            list_level=list_level,
                        )
                        blocks.extend(ordered_blocks)
                        if title is None and ordered_blocks:
                            first_text_block = next((block for block in ordered_blocks if block.kind != "image" and block.text.strip()), None)
                            if first_text_block is not None:
                                title = heading_path() or detect_title_like_line(first_text_block.text) or path.stem
                            elif any(block.kind == "image" for block in ordered_blocks):
                                title = heading_path() or path.stem
                    else:
                        image_blocks = extract_docx_inline_image_blocks_from_paragraph(
                            element,
                            archive,
                            path,
                            rel_targets,
                            heading_path(),
                            section,
                        )
                        blocks.extend(image_blocks)
                        if title is None and image_blocks:
                            title = heading_path() or path.stem
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
    converted_docx = convert_document_with_office(path, "docx")
    if converted_docx is not None:
        try:
            title, blocks = parse_docx(converted_docx)
            if blocks:
                return title, blocks
        except Exception:  # noqa: BLE001
            pass

    converted_html = convert_document_with_office(path, "html")
    if converted_html is not None:
        try:
            title, blocks = parse_html(converted_html)
            if blocks:
                return title, blocks
        except Exception:  # noqa: BLE001
            pass

    text = extract_doc_text_with_textutil(path)
    blocks = [Block(kind="paragraph", text=paragraph) for paragraph in split_paragraphs(text)]
    return None, blocks


def parse_ppt(path: Path, emit: Optional[ProgressEmitter] = None, request_id: str = "") -> Tuple[Optional[str], List[Block]]:
    converted_pdf = convert_document_with_office(path, "pdf")
    if converted_pdf is None:
        raise parser_error(
            "parse_failed",
            "office conversion failed for ppt/pptx",
            details=f"failed to convert {path} to pdf via LibreOffice",
        )

    try:
        title, blocks, _ocr_tasks = parse_pdf(converted_pdf, emit=emit, request_id=request_id)
        return title, blocks
    except Exception as exc:  # noqa: BLE001
        raise parser_error(
            "parse_failed",
            "ppt/pptx pdf parsing failed",
            details=str(exc),
        ) from exc


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
    related_parts = getattr(getattr(document, "part", None), "related_parts", {}) or {}

    def heading_path() -> Optional[str]:
        if not heading_stack:
            return None
        return " > ".join(heading for _, heading in heading_stack)

    for child in document.element.body.iterchildren():
        if child.tag.endswith("}p"):
            paragraph = Paragraph(child, document)
            style_name = getattr(getattr(paragraph, "style", None), "name", "") or ""
            text = clean_docx_text(paragraph.text)
            section = "body" if seen_body_heading else "frontmatter"
            image_blocks = extract_docx_inline_image_blocks_from_python_docx_paragraph(
                paragraph,
                path,
                heading_path(),
                section,
                related_parts,
            )

            if style_name.lower().startswith("heading"):
                match = re.search(r"heading\s*(\d+)", style_name.lower())
                level = int(match.group(1)) if match else 1
                if title is None and text:
                    title = text
                    blocks.append(
                        Block(
                            kind="heading",
                            text=text,
                            level=level,
                            section="frontmatter",
                            markdown=f"{'#' * level} {text}",
                        )
                    )
                    blocks.extend(image_blocks)
                    continue
                while heading_stack and heading_stack[-1][0] >= level:
                    heading_stack.pop()
                heading_stack.append((level, text or heading_path() or path.stem))
                seen_body_heading = True
                heading_text = text or heading_path() or path.stem
                blocks.append(
                    Block(
                        kind="heading",
                        text=heading_text,
                        level=level,
                        section="body",
                        markdown=f"{'#' * level} {heading_text}",
                    )
                )
                blocks.extend(image_blocks)
            else:
                list_kind, list_level = parse_docx_list_info_from_style(style_name)
                if text:
                    ordered_blocks = extract_docx_ordered_inline_blocks_from_python_docx_paragraph(
                        paragraph,
                        path,
                        heading_path(),
                        section,
                        related_parts,
                        list_kind=list_kind,
                        list_level=list_level,
                    )
                    blocks.extend(ordered_blocks)
                    if title is None and ordered_blocks:
                        first_text_block = next((block for block in ordered_blocks if block.kind != "image" and block.text.strip()), None)
                        if first_text_block is not None:
                            title = heading_path() or detect_title_like_line(first_text_block.text) or path.stem
                        elif any(block.kind == "image" for block in ordered_blocks):
                            title = heading_path() or path.stem
                else:
                    image_blocks = extract_docx_inline_image_blocks_from_python_docx_paragraph(
                        paragraph,
                        path,
                        heading_path(),
                        section,
                        related_parts,
                    )
                    blocks.extend(image_blocks)
                    if title is None and image_blocks:
                        title = heading_path() or path.stem

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

    stderr_text = normalize_whitespace(result.stderr)
    if stderr_text and looks_like_textutil_error_output(stderr_text):
        raise parser_error(
            "parse_failed",
            "textutil returned an error instead of document text",
            details=stderr_text,
        )

    text = normalize_whitespace(result.stdout)
    if looks_like_textutil_error_output(text):
        raise parser_error(
            "parse_failed",
            "textutil returned an error instead of document text",
            details=text,
        )
    if not text:
        details = stderr_text or f"textutil produced empty output for {path}"
        raise parser_error(
            "parse_failed",
            "textutil produced empty document text",
            details=details,
        )

    return text


def looks_like_textutil_error_output(text: str) -> bool:
    lowered = normalize_whitespace(text).lower()
    return lowered.startswith("error reading ") or "the file isn’t in the correct format" in lowered or "the file isn't in the correct format" in lowered


PDF_IMAGE_CACHE_ROOT = Path(tempfile.gettempdir()) / "seekmind-pdf-media"



def eprint(message: str) -> None:
    print(message, file=sys.stderr, flush=True)


def pdf_debug_log(message: str) -> None:
    eprint(f"[SeekMind][PDF] {message}")



def pdf_image_cache_dir(path: Path) -> Path:
    digest = hashlib.sha256(str(path).encode("utf-8")).hexdigest()[:16]
    return PDF_IMAGE_CACHE_ROOT / digest



def pdf_image_extraction_enabled() -> bool:
    return os.environ.get("SeekMind_ENABLE_SYSTEM_PDF_IMAGES", "").strip() == "1"



def vision_ocr_binary() -> Optional[str]:
    bundled = env_value("SEEKMIND_VISION_OCR_BIN", "SeekMind_VISION_OCR_BIN") or ""
    if bundled and Path(bundled).is_file():
        return bundled

    # 修复：沙盒版 OCR 不能再默认依赖系统路径，这里优先使用随 App 打包的 Vision OCR helper。
    bundled_binary = "vision-ocr.exe" if sys.platform.startswith("win") else "vision-ocr"
    for candidate in (
        shutil.which(bundled_binary),
        str(Path(__file__).resolve().parents[2] / "src-tauri" / "app-resources" / "ocr" / bundled_binary),
    ):
        if candidate and Path(candidate).is_file():
            return candidate

    return None


def pdf_ocr_enabled() -> bool:
    return (env_value("SEEKMIND_DISABLE_PDF_OCR", "SeekMind_DISABLE_PDF_OCR") or "") != "1" and vision_ocr_binary() is not None


@lru_cache(maxsize=1)
def vision_ocr_languages() -> list[str]:
    raw = os.environ.get("SEEKMIND_VISION_OCR_LANGS", "").strip() or "zh-Hans,en-US"
    languages = [part.strip() for part in raw.split(",") if part.strip()]
    return languages or ["zh-Hans", "en-US"]


def contains_cjk(text: str) -> bool:
    return any("\u4e00" <= c <= "\u9fff" or "\u3400" <= c <= "\u4dbf" for c in text)



def ocr_pdf_page_text(path: Path, page_index: int) -> Optional[str]:
    if not pdf_ocr_enabled():
        pdf_debug_log(f"page={page_index} OCR skipped: disabled or Vision OCR helper missing")
        return None

    try:
        import fitz  # type: ignore
    except Exception as exc:
        pdf_debug_log(f"page={page_index} OCR skipped: fitz import failed: {exc}")
        return None

    try:
        output_dir = pdf_image_cache_dir(path) / "ocr"
        output_dir.mkdir(parents=True, exist_ok=True)
        render_path = output_dir / f"page-{page_index}.png"
        doc = fitz.open(str(path))
        page = doc.load_page(page_index - 1)
        pixmap = page.get_pixmap(matrix=fitz.Matrix(2, 2), alpha=False)
        pixmap.save(str(render_path))
        pdf_debug_log(f"page={page_index} OCR render saved: {render_path}")
    except Exception as exc:
        pdf_debug_log(f"page={page_index} OCR render failed: {exc}")
        return None

    langs = ",".join(vision_ocr_languages())
    binary = vision_ocr_binary()
    if not binary:
        pdf_debug_log(f"page={page_index} OCR skipped: bundled Vision OCR helper missing")
        return None
    try:
        result = subprocess.run(
            [binary, "--image", str(render_path), "--langs", langs],
            check=False,
            capture_output=True,
            text=True,
            timeout=120,
        )
    except Exception as exc:
        pdf_debug_log(f"page={page_index} OCR execution failed: {exc}")
        return None
    if result.returncode != 0:
        pdf_debug_log(
            f"page={page_index} OCR failed code={result.returncode} stderr={normalize_whitespace(result.stderr)[:240]}"
        )
        return None
    text = normalize_whitespace(result.stdout)
    if not text:
        return None
    if looks_like_pdf_text_layer_noise(text):
        pdf_debug_log(f"page={page_index} OCR output rejected as noise")
        return None
    if not is_meaningful_text(text):
        return None
    return text


def render_pdf_page_preview_block(path: Path, heading: Optional[str], page_index: int) -> Optional[Block]:
    try:
        import fitz  # type: ignore
    except Exception as exc:
        pdf_debug_log(f"page={page_index} preview skipped: fitz import failed: {exc}")
        return None

    try:
        output_dir = pdf_image_cache_dir(path) / "page-preview"
        output_dir.mkdir(parents=True, exist_ok=True)
        preview_path = output_dir / f"page-{page_index}.png"
        doc = fitz.open(str(path))
        page = doc.load_page(page_index - 1)
        pixmap = page.get_pixmap(matrix=fitz.Matrix(2, 2), alpha=False)
        pixmap.save(str(preview_path))
        pdf_debug_log(f"page={page_index} preview saved: {preview_path}")
    except Exception as exc:
        pdf_debug_log(f"page={page_index} preview failed: {exc}")
        return None

    if not preview_path.exists() or preview_path.stat().st_size <= 0:
        pdf_debug_log(f"page={page_index} preview missing or empty: {preview_path}")
        return None

    label = f"PDF 页面 {page_index}"
    caption = "PDF 页面预览"
    return Block(
        kind="image",
        text=label,
        heading=heading or path.stem,
        page_no=page_index,
        markdown=f"![{label}]({preview_path})",
        html=build_img_html(str(preview_path), label, caption),
        asset_path=str(preview_path),
        alt_text=label,
        caption=caption,
        ocr_text=None,
    )



def extract_pdf_image_blocks(path: Path, heading: Optional[str], page_index: Optional[int] = None) -> List[Block]:
    image_blocks: List[Block] = []
    output_dir = pdf_image_cache_dir(path)
    output_dir.mkdir(parents=True, exist_ok=True)
    pdf_debug_log(f"extract image blocks start path={path.name} page={page_index or 'all'} cache={output_dir}")

    try:
        import fitz  # type: ignore
    except Exception:
        fitz = None

    if fitz is not None:
        try:
            doc = fitz.open(str(path))
            pages = [(page_index, doc.load_page(page_index - 1))] if page_index is not None else list(enumerate(doc, start=1))
            pdf_debug_log(f"fitz image scan pages={len(pages)} path={path.name}")
            for page_index, page in pages:
                images = page.get_images(full=True)
                pdf_debug_log(f"page={page_index} fitz found {len(images)} embedded images")
                for image_index, image in enumerate(images, start=1):
                    xref = image[0]
                    try:
                        extracted = doc.extract_image(xref)
                    except Exception as exc:  # noqa: BLE001
                        pdf_debug_log(f"page={page_index} image={image_index} xref={xref} extract failed: {exc}")
                        continue
                    image_bytes = extracted.get("image") if isinstance(extracted, dict) else None
                    if not image_bytes:
                        pdf_debug_log(f"page={page_index} image={image_index} xref={xref} extracted empty image bytes")
                        continue
                    ext = normalize_extension(str(extracted.get("ext") or "png").lower().lstrip("."))
                    image_path = output_dir / f"page-{page_index}-{image_index}.{ext}"
                    try:
                        image_path.write_bytes(image_bytes)
                    except Exception as exc:  # noqa: BLE001
                        pdf_debug_log(f"page={page_index} image={image_index} write failed: {exc}")
                        continue
                    label = f"PDF 图片 {page_index}-{image_index}"
                    caption = "PDF 图片预览"
                    pdf_debug_log(f"page={page_index} image={image_index} extracted -> {image_path}")
                    image_blocks.append(
                        Block(
                            kind="image",
                            text=label,
                            heading=heading or path.stem,
                            page_no=page_index,
                            markdown=f"![{label}]({image_path})",
                            html=build_img_html(str(image_path), label, caption),
                            asset_path=str(image_path),
                            alt_text=label,
                            caption=caption,
                            ocr_text=None,
                        )
                    )
            if image_blocks:
                pdf_debug_log(f"fitz extraction success path={path.name} page={page_index or 'all'} images={len(image_blocks)}")
                return image_blocks
        except Exception as exc:
            pdf_debug_log(f"fitz extraction failed path={path.name} page={page_index or 'all'} err={exc}")
            image_blocks = []

    if not pdf_image_extraction_enabled():
        pdf_debug_log(f"system pdfimages disabled path={path.name}")
        return image_blocks

    total_pages = 0
    try:
        from pypdf import PdfReader  # type: ignore

        reader = PdfReader(str(path))
        total_pages = len(reader.pages)
    except Exception:
        total_pages = 0

    if total_pages <= 0:
        pdf_debug_log(f"pdfimages fallback skipped path={path.name}: no page count")
        return image_blocks

    page_indices = [page_index] if page_index is not None else list(range(1, total_pages + 1))
    for page_index in page_indices:
        page_prefix = output_dir / f"page-{page_index}"
        pdf_debug_log(f"pdfimages extracting page={page_index} prefix={page_prefix}")
        for existing in output_dir.glob(f"{page_prefix.name}*"):
            try:
                existing.unlink()
            except Exception:  # noqa: BLE001
                pass

        try:
            result = subprocess.run(
                [
                    "pdfimages",
                    "-png",
                    "-p",
                    "-f",
                    str(page_index),
                    "-l",
                    str(page_index),
                    "-print-filenames",
                    str(path),
                    str(page_prefix),
                ],
                check=False,
                capture_output=True,
                text=True,
                timeout=60,
            )
        except FileNotFoundError:
            return image_blocks
        except subprocess.TimeoutExpired:
            continue
        except Exception:  # noqa: BLE001
            continue

        if result.returncode != 0:
            continue

        output_files: List[Path] = []
        for raw_line in result.stdout.splitlines():
            candidate = normalize_whitespace(raw_line)
            if not candidate:
                continue
            candidate_path = Path(candidate)
            if not candidate_path.is_absolute():
                candidate_path = Path(candidate)
            if candidate_path.exists():
                output_files.append(candidate_path)

        if not output_files:
            output_files = sorted(
                (item for item in output_dir.glob(f"{page_prefix.name}*") if item.is_file()),
                key=lambda item: item.stat().st_mtime,
            )

        if not output_files:
            pdf_debug_log(f"pdfimages page={page_index} produced no output files")
            continue

        for image_index, image_path in enumerate(output_files, start=1):
            if not image_path.exists() or image_path.stat().st_size <= 0:
                continue
            label = f"PDF 图片 {page_index}-{image_index}"
            caption = "PDF 图片预览"
            pdf_debug_log(f"pdfimages page={page_index} extracted -> {image_path}")
            image_blocks.append(
                Block(
                    kind="image",
                    text=label,
                    heading=heading or path.stem,
                    page_no=page_index,
                    markdown=f"![{label}]({image_path})",
                    html=build_img_html(str(image_path), label, caption),
                    asset_path=str(image_path),
                    alt_text=label,
                    caption=caption,
                    ocr_text=None,
                )
            )

    return image_blocks



def parse_pdf(
    path: Path,
    emit: Optional[ProgressEmitter] = None,
    request_id: str = "",
) -> Tuple[Optional[str], List[Block], List[PdfOcrTask]]:
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

    title: Optional[str] = None
    blocks: List[Block] = []
    ocr_tasks: List[PdfOcrTask] = []
    pdf_debug_log(f"parse start path={path.name}")

    try:
        from pypdf import PdfReader  # type: ignore

        reader = PdfReader(str(path))
        total_pages = len(reader.pages)
        pdf_debug_log(f"pdf reader opened path={path.name} pages={total_pages}")
        progress("extract", f"正在解析 PDF，共 {total_pages} 页", 10, path.name, total_pages, 0)
        ocr_pages = 0
        scanned_candidates = 0
        text_pages = 0
        skipped_pages = 0
        for page_index, page in enumerate(reader.pages, start=1):
            page_text = normalize_whitespace((page.extract_text() or "").replace("\x0c", "\n\n"))
            meaningful = is_meaningful_text(page_text)
            noisy_text_layer = looks_like_pdf_text_layer_noise(page_text)
            pdf_debug_log(
                f"page={page_index} text_len={len(page_text)} meaningful={meaningful} noisy={noisy_text_layer}"
            )
            if page_text and meaningful and not noisy_text_layer:
                text_pages += 1
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
            else:
                task = build_pdf_ocr_task(page_index, page_text, meaningful, noisy_text_layer)
                if task is not None:
                    scanned_candidates += 1
                    ocr_tasks.append(task)
                    progress(
                        "extract",
                        task.message,
                        15,
                        path.name,
                        total_pages,
                        page_index,
                        warning=task.warning,
                    )

            page_image_blocks = extract_pdf_image_blocks(path, title or path.stem, page_index)
            pdf_debug_log(f"page={page_index} image_block_count={len(page_image_blocks)}")
            if page_image_blocks:
                blocks.extend(page_image_blocks)
            else:
                page_preview = render_pdf_page_preview_block(path, title or path.stem, page_index)
                if page_preview is not None:
                    pdf_debug_log(f"page={page_index} page preview fallback appended")
                    blocks.append(page_preview)
                else:
                    pdf_debug_log(f"page={page_index} no image block and no page preview")

        if ocr_tasks:
            pdf_debug_log(f"pdf ocr queue collected path={path.name} tasks={len(ocr_tasks)}")
            progress(
                "ocr_queue",
                f"已排队 {len(ocr_tasks)} 个 OCR 任务",
                20,
                path.name,
                total_pages,
                len(ocr_tasks),
            )
        for queue_index, task in enumerate(ocr_tasks, start=1):
            page_index = task.page_index
            if not pdf_ocr_enabled():
                skipped_pages += 1
                ocr_reason = "Vision OCR helper 未安装" if vision_ocr_binary() is None else "SEEKMIND_DISABLE_PDF_OCR=1"
                task.status = "skipped"
                task.error = ocr_reason
                progress(
                    "ocr",
                    f"第 {page_index} 页 OCR 跳过（{ocr_reason}）",
                    min(60, 20 + int(page_index / max(total_pages, 1) * 30)),
                    path.name,
                    total_pages,
                    page_index,
                    warning=f"PDF OCR 未启用（{ocr_reason}）",
                )
                continue

            # 修复：OCR 先收集为队列，再统一执行，后续可以无缝把这一步挪到后台任务。
            progress(
                "ocr_queue",
                f"正在处理 OCR 队列第 {queue_index}/{len(ocr_tasks)} 项",
                min(70, 30 + int(queue_index / max(len(ocr_tasks), 1) * 35)),
                path.name,
                len(ocr_tasks),
                queue_index,
                warning=task.warning,
            )
            progress(
                "ocr",
                f"正在识别第 {page_index} 页 PDF 图片文字",
                min(80, 20 + int(page_index / max(total_pages, 1) * 50)),
                path.name,
                total_pages,
                page_index,
            )
            ocr_text = ocr_pdf_page_text(path, page_index)
            if ocr_text:
                ocr_pages += 1
                task.status = "completed"
                task.ocr_text = ocr_text
                progress("ocr", f"第 {page_index} 页 OCR 识别成功", 25, path.name, total_pages, page_index)
                if title is None:
                    title = detect_title_like_line(ocr_text) or path.stem
                blocks.append(
                    Block(
                        kind="paragraph",
                        text=ocr_text,
                        heading=title or path.stem,
                        page_no=page_index,
                        ocr_text=ocr_text,
                    )
                )
            else:
                skipped_pages += 1
                task.status = "failed"
                task.error = "OCR 返回空"
                progress("ocr", f"第 {page_index} 页 OCR 识别无结果", 20, path.name, total_pages, page_index, warning="OCR 返回空")

        if blocks:
            image_count = sum(1 for block in blocks if block.kind == "image")
            ocr_block_count = sum(1 for block in blocks if block.ocr_text)
            pdf_debug_log(f"parse done path={path.name} blocks={len(blocks)} images={image_count} ocr_blocks={ocr_block_count}")
            summary = f"共 {total_pages} 页：pypdf 文本 {text_pages} 页，扫描候选 {scanned_candidates} 页"
            if ocr_pages:
                summary += f"，OCR {ocr_pages} 页"
            if skipped_pages:
                summary += f"，跳过 {skipped_pages} 页"
            if image_count:
                summary += f"，图片 {image_count} 个"
            if scanned_candidates > 0 and ocr_pages == 0:
                # 修复：把“扫描候选但 OCR 未产出”明确反馈到进度摘要，方便上层判断是缺语言包还是 OCR 没触发。
                progress("ocr", f"扫描候选 {scanned_candidates} 页，但 OCR 未产出结果", 30, path.name, total_pages, scanned_candidates, warning="OCR 未产出结果")
            progress("done", summary, 100, path.name, total_pages, total_pages)
            if image_count:
                progress("image", f"已提取 {image_count} 个 PDF 图片", 85, path.name, image_count, image_count)
            return title or path.stem, blocks, ocr_tasks
    except Exception as exc:  # noqa: BLE001
        parse_error = exc
    else:
        parse_error = None

    pdftotext = _extract_pdf_with_pdftotext(path)
    if pdftotext is not None:
        title, blocks = pdftotext
        image_blocks = extract_pdf_image_blocks(path, title or path.stem)
        if image_blocks:
            progress("image", f"已提取 {len(image_blocks)} 个 PDF 图片", 85, path.name, len(image_blocks), len(image_blocks))
        else:
            try:
                from pypdf import PdfReader  # type: ignore
                reader = PdfReader(str(path))
                for page_index in range(1, len(reader.pages) + 1):
                    page_preview = render_pdf_page_preview_block(path, title or path.stem, page_index)
                    if page_preview is not None:
                        image_blocks.append(page_preview)
            except Exception:
                pass
        blocks.extend(image_blocks)
        progress("done", f"PDF 解析完成：{path.name}", 100, path.name, len(blocks), len(blocks))
        return title or path.stem, blocks, []

    image_blocks = extract_pdf_image_blocks(path, path.stem)
    if image_blocks:
        progress("image", f"已提取 {len(image_blocks)} 个 PDF 图片", 85, path.name, len(image_blocks), len(image_blocks))
        progress("done", f"PDF 解析完成：{path.name}", 100, path.name, len(image_blocks), len(image_blocks))
        return path.stem, image_blocks, []

    page_preview_blocks: List[Block] = []
    try:
        from pypdf import PdfReader  # type: ignore
        reader = PdfReader(str(path))
        for page_index in range(1, len(reader.pages) + 1):
            page_preview = render_pdf_page_preview_block(path, path.stem, page_index)
            if page_preview is not None:
                page_preview_blocks.append(page_preview)
    except Exception:
        page_preview_blocks = []

    if page_preview_blocks:
        progress("image", f"已生成 {len(page_preview_blocks)} 个 PDF 页面预览", 85, path.name, len(page_preview_blocks), len(page_preview_blocks))
        progress("done", f"PDF 解析完成：{path.name}", 100, path.name, len(page_preview_blocks), len(page_preview_blocks))
        return path.stem, page_preview_blocks, []

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

    raw_text = result.stdout.replace("\r\n", "\n").strip()
    if not raw_text or not is_meaningful_text(raw_text) or looks_like_pdf_text_layer_noise(raw_text):
        return None

    blocks: List[Block] = []
    title: Optional[str] = None

    for page_index, page_text in enumerate(raw_text.split("\f"), start=1):
        paragraphs = split_paragraphs(page_text.replace("\x0c", "\n\n"))
        for paragraph in paragraphs:
            if not paragraph:
                continue
            if not is_meaningful_text(paragraph):
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
        blocks.extend(extract_pdf_image_blocks(path, title or path.stem, page_index))

    if not blocks:
        return None

    return title or path.stem, blocks


def build_chunks(
    blocks: Sequence[Block],
    path: Path,
    options: ParserOptions,
    emit: Optional[ProgressEmitter] = None,
    request_id: str = "",
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
                            request_id=request_id,
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

    def emit_pending() -> None:
        nonlocal pending
        if pending is not None:
            merged.append(pending)
            pending = None

    for block in blocks:
        if block.kind in {"heading", "code", "image"}:
            emit_pending()
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
                language=block.language,
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
                language=pending.language,
                markdown=pending.markdown,
                html=pending.html,
                asset_path=pending.asset_path,
                alt_text=pending.alt_text,
                caption=pending.caption,
                ocr_text=pending.ocr_text,
            )
        else:
            emit_pending()
            pending = Block(
                kind=block.kind,
                text=text,
                heading=block.heading,
                page_no=block.page_no,
                section=block.section,
                level=block.level,
                language=block.language,
                markdown=block.markdown,
                html=block.html,
                asset_path=block.asset_path,
                alt_text=block.alt_text,
                caption=block.caption,
                ocr_text=block.ocr_text,
            )

    emit_pending()
    return merged


def normalize_blocks(blocks: Sequence[Block]) -> List[Block]:
    normalized: List[Block] = []
    for block in blocks:
        if block.kind == "code":
            text = block.text.rstrip("\n")
            if not text.strip():
                continue
            normalized.append(
                Block(
                    kind=block.kind,
                    text=text,
                    heading=block.heading,
                    page_no=block.page_no,
                    section=block.section,
                    level=block.level,
                    language=block.language,
                    markdown=block.markdown,
                    html=block.html,
                    asset_path=block.asset_path,
                    alt_text=block.alt_text,
                    caption=block.caption,
                    ocr_text=block.ocr_text,
                )
            )
            continue

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
                language=block.language,
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


def normalize_pptx_target(target: str) -> str:
    cleaned = normalize_whitespace(target).replace("\\", "/")
    cleaned = cleaned.lstrip("/")
    while cleaned.startswith("../"):
        cleaned = cleaned[3:]
    if cleaned.startswith("ppt/"):
        return cleaned
    if cleaned:
        return f"ppt/{cleaned}"
    return ""


def load_pptx_relationship_targets(archive: zipfile.ZipFile, rel_path: str) -> dict[str, str]:
    try:
        rel_xml = archive.read(rel_path)
    except KeyError:
        return {}

    try:
        root = ElementTree.fromstring(rel_xml)
    except Exception:  # noqa: BLE001
        return {}

    targets: dict[str, str] = {}
    for rel in root:
        if strip_ns(rel.tag) != "Relationship":
            continue
        rel_id = rel.attrib.get("Id", "").strip()
        target = rel.attrib.get("Target", "").strip()
        if not rel_id or not target:
            continue
        if rel.attrib.get("TargetMode", "").lower() == "external":
            continue
        targets[rel_id] = target
    return targets


def load_pptx_slide_paths(archive: zipfile.ZipFile) -> List[str]:
    try:
        presentation_xml = archive.read("ppt/presentation.xml")
        presentation_root = ElementTree.fromstring(presentation_xml)
    except Exception:  # noqa: BLE001
        presentation_root = None

    rel_targets = load_pptx_relationship_targets(archive, "ppt/_rels/presentation.xml.rels")
    if presentation_root is not None:
        slide_paths: List[str] = []
        for slide_id in presentation_root.findall(".//p:sldId", PPTX_NAMESPACES):
            rel_id = slide_id.attrib.get(f"{{{PPTX_NAMESPACES['r']}}}id", "").strip()
            target = rel_targets.get(rel_id, "")
            archive_path = normalize_pptx_target(target)
            if archive_path:
                slide_paths.append(archive_path)
        if slide_paths:
            return slide_paths

    candidates = sorted(
        (
            Path(name)
            for name in archive.namelist()
            if name.startswith("ppt/slides/slide") and name.endswith(".xml") and "/_rels/" not in name
        ),
        key=lambda item: int(re.search(r"slide(\d+)\.xml$", item.name).group(1)) if re.search(r"slide(\d+)\.xml$", item.name) else 0,
    )
    return [str(path) for path in candidates]


def pptx_shape_placeholder_type(shape: ElementTree.Element) -> str:
    placeholder = shape.find("./p:nvSpPr/p:nvPr/p:ph", PPTX_NAMESPACES)
    if placeholder is None:
        return ""
    return normalize_whitespace(placeholder.attrib.get("type", ""))


def extract_pptx_text_paragraphs(container: ElementTree.Element) -> List[str]:
    paragraphs: List[str] = []
    for paragraph in container.findall(".//a:p", PPTX_NAMESPACES):
        parts: List[str] = []
        for node in paragraph.iter():
            tag = strip_ns(node.tag)
            if tag == "t":
                parts.append(node.text or "")
            elif tag == "tab":
                parts.append("\t")
            elif tag == "br":
                parts.append("\n")
        text = clean_docx_text("".join(parts))
        if text:
            paragraphs.append(text)
    return paragraphs


def extract_pptx_table_rows(table: ElementTree.Element) -> List[List[str]]:
    rows: List[List[str]] = []
    for row in table.findall("./a:tr", PPTX_NAMESPACES):
        cells: List[str] = []
        for cell in row.findall("./a:tc", PPTX_NAMESPACES):
            text = " ".join(extract_pptx_text_paragraphs(cell))
            text = normalize_whitespace(text)
            text = clean_docx_text(text)
            if text:
                cells.append(text)
        row_text = normalize_whitespace(" | ".join(cells))
        if row_text:
            rows.append(cells)
    return rows


def resolve_pptx_image_asset_path(
    document_path: Path,
    source_name: str,
    data: bytes,
    content_type: Optional[str] = None,
) -> tuple[str, Optional[str]]:
    stored_path = Path(store_pptx_media_bytes(document_path, source_name, data))
    if is_docx_vector_image_source(source_name, content_type):
        converted_path = try_convert_docx_vector_image_to_png(document_path, stored_path)
        if converted_path is not None:
            return str(converted_path), None
        return str(stored_path), docx_vector_image_note(source_name, content_type)
    return str(stored_path), None


def build_pptx_image_block(
    asset_path: str,
    slide_index: int,
    alt_text: Optional[str] = None,
    caption: Optional[str] = None,
    preview_note: Optional[str] = None,
) -> Block:
    label = normalize_whitespace(alt_text or caption or image_label_from_path(asset_path) or "image")
    note = normalize_whitespace(preview_note or "")
    final_caption = caption or (note if note else None)
    return Block(
        kind="image",
        text=label,
        heading=f"Slide {slide_index}",
        page_no=slide_index,
        section="body",
        markdown=f"![{label}]({asset_path})",
        html=build_img_html(asset_path, alt_text or label, final_caption or ""),
        asset_path=asset_path or None,
        alt_text=alt_text or None,
        caption=final_caption,
        ocr_text=note or None,
    )


def extract_pptx_slide_blocks(
    archive: zipfile.ZipFile,
    slide_path: str,
    document_path: Path,
    slide_index: int,
) -> Tuple[Optional[str], List[Block]]:
    blocks: List[Block] = []
    title: Optional[str] = None
    heading = f"Slide {slide_index}"
    slide_rels = load_pptx_relationship_targets(archive, f"{Path(slide_path).parent}/_rels/{Path(slide_path).name}.rels")

    try:
        slide_xml = archive.read(slide_path)
        root = ElementTree.fromstring(slide_xml)
    except Exception:  # noqa: BLE001
        return None, blocks

    def emit_text_blocks(container: ElementTree.Element) -> None:
        nonlocal title
        placeholder_type = pptx_shape_placeholder_type(container)
        paragraphs = extract_pptx_text_paragraphs(container)
        if not paragraphs:
            return

        is_title_shape = placeholder_type in {"title", "ctrTitle", "titleTx"}
        for index, paragraph_text in enumerate(paragraphs):
            if is_title_shape and index == 0:
                blocks.append(
                    Block(
                        kind="heading",
                        text=paragraph_text,
                        heading=heading,
                        page_no=slide_index,
                        section="body",
                        level=1,
                    )
                )
                title = paragraph_text
                continue
            blocks.append(
                Block(
                    kind="paragraph",
                    text=paragraph_text,
                    heading=heading,
                    page_no=slide_index,
                    section="body",
                    markdown=paragraph_text,
                )
            )
            if title is None:
                title = paragraph_text

    for child in root.findall("./p:cSld/p:spTree/*", PPTX_NAMESPACES):
        tag = strip_ns(child.tag)
        if tag == "nvGrpSpPr" or tag == "grpSpPr":
            continue
        if tag == "sp":
            emit_text_blocks(child)
            continue
        if tag == "pic":
            rel_id = ""
            blip = child.find(".//a:blip", PPTX_NAMESPACES)
            if blip is not None:
                rel_id = blip.attrib.get(f"{{{PPTX_NAMESPACES['r']}}}embed") or blip.attrib.get(
                    f"{{{PPTX_NAMESPACES['r']}}}link", ""
                )
            if not rel_id:
                continue
            target = slide_rels.get(rel_id, "")
            archive_path = normalize_pptx_target(target)
            if not archive_path:
                continue
            try:
                data = archive.read(archive_path)
            except KeyError:
                continue
            content_type = ""
            image_path = Path(archive_path)
            suffix = image_path.suffix.lower()
            if suffix:
                content_type = {
                    ".png": "image/png",
                    ".jpg": "image/jpeg",
                    ".jpeg": "image/jpeg",
                    ".gif": "image/gif",
                    ".bmp": "image/bmp",
                    ".emf": "image/emf",
                    ".wmf": "image/wmf",
                    ".svg": "image/svg+xml",
                }.get(suffix, "")
            c_nv_pr = child.find("./p:nvPicPr/p:cNvPr", PPTX_NAMESPACES)
            alt_text = normalize_whitespace(c_nv_pr.attrib.get("descr", "")) if c_nv_pr is not None else ""
            caption = normalize_whitespace(c_nv_pr.attrib.get("name", "")) if c_nv_pr is not None else ""
            asset_path, preview_note = resolve_pptx_image_asset_path(
                document_path,
                archive_path,
                data,
                content_type or None,
            )
            blocks.append(
                build_pptx_image_block(
                    asset_path,
                    slide_index,
                    alt_text=alt_text or None,
                    caption=caption or None,
                    preview_note=preview_note,
                )
            )
            continue
        if tag == "graphicFrame":
            table = child.find(".//a:tbl", PPTX_NAMESPACES)
            if table is None:
                continue
            rows = extract_pptx_table_rows(table)
            if rows:
                text, markdown, html_table = table_rows_to_formats(rows)
                if text:
                    blocks.append(
                        Block(
                            kind="table",
                            text=text,
                            heading=heading,
                            page_no=slide_index,
                            section="body",
                            markdown=markdown,
                            html=html_table,
                        )
                    )
                    if title is None:
                        title = text
            continue
        if tag in {"cxnSp", "grpSp"} and child.findall(".//a:t", PPTX_NAMESPACES):
            emit_text_blocks(child)

    return title, blocks


def parse_pptx(path: Path) -> Tuple[Optional[str], List[Block]]:
    try:
        with zipfile.ZipFile(path) as archive:
            slide_paths = load_pptx_slide_paths(archive)
            blocks: List[Block] = []
            title: Optional[str] = None
            for index, slide_path in enumerate(slide_paths, start=1):
                slide_title, slide_blocks = extract_pptx_slide_blocks(archive, slide_path, path, index)
                if title is None and slide_title:
                    title = slide_title
                blocks.extend(slide_blocks)
    except Exception as exc:  # noqa: BLE001
        raise parser_error("parse_failed", "pptx parsing failed", details=str(exc)) from exc

    if not blocks:
        raise parser_error("parse_failed", "pptx parsing produced no content", details=str(path))

    return title or path.stem, blocks


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


def is_meaningful_text(text: str) -> bool:
    stripped = text.strip()
    if not stripped:
        return False

    total = len(stripped)
    alpha = sum(1 for c in stripped if c.isalpha())
    cjk = sum(1 for c in stripped if '\u4e00' <= c <= '\u9fff' or '\u3400' <= c <= '\u4dbf')
    meaningful = alpha + cjk
    if meaningful == 0:
        return False

    words = stripped.split()
    if not words:
        return False

    avg_word_len = sum(len(w) for w in words) / len(words)
    if avg_word_len > 25 and cjk == 0:
        return False

    meaningful_ratio = meaningful / total
    if meaningful_ratio < 0.25:
        return False

    vowel_words = sum(1 for w in words if len(w) > 1 and any(v in w.lower() for v in "aeiou"))
    if len(words) >= 3 and vowel_words / len(words) < 0.15 and cjk == 0:
        return False

    return True


def looks_like_pdf_text_layer_noise(text: str) -> bool:
    cleaned = normalize_whitespace(text)
    if not cleaned:
        return False

    cjk = sum(1 for c in cleaned if "\u4e00" <= c <= "\u9fff" or "\u3400" <= c <= "\u4dbf")
    if cjk > 0:
        return False

    tokens = [token for token in re.split(r"\s+", cleaned) if token]
    if len(tokens) < 8:
        return False

    alpha_tokens = [token for token in tokens if re.search(r"[A-Za-z]", token)]
    if not alpha_tokens:
        return False

    normalized_letters = [re.sub(r"[^A-Za-z]", "", token) for token in alpha_tokens]
    pure_letters = [token for token in normalized_letters if token]
    if not pure_letters:
        return False

    short_alpha_ratio = sum(1 for token in pure_letters if len(token) <= 4) / len(pure_letters)
    uppercase_ratio = sum(1 for token in pure_letters if token.isupper() and len(token) >= 3) / len(pure_letters)
    lowercase_ratio = sum(1 for token in pure_letters if token.islower() and len(token) >= 4) / len(pure_letters)
    mixed_shape_ratio = sum(1 for token in alpha_tokens if not re.fullmatch(r"[A-Za-z]+", token)) / len(alpha_tokens)

    if short_alpha_ratio >= 0.7 and uppercase_ratio >= 0.35 and lowercase_ratio <= 0.2 and mixed_shape_ratio >= 0.25:
        return True

    return False


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
