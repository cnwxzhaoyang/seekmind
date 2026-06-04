"""
@author MorningSun
@CreatedDate 2026/06/03
@Description Python sidecar RAG 的 SQLite 只读访问层。
"""

from __future__ import annotations

import json
import sqlite3
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Dict, Iterable, List, Optional, Sequence


def _escape_like(value: str) -> str:
    return value.replace("\\", "\\\\").replace("%", "\\%").replace("_", "\\_")


@dataclass
class RagChunkRecord:
    chunk_id: str
    document_id: str
    file_name: str
    path: str
    ext: str
    heading: str
    snippet: str
    paragraph: Optional[int]
    page: Optional[int]
    modified: str
    modified_at: int
    score: float
    block_indexes_json: str


@dataclass
class RagBlockRecord:
    document_id: str
    block_index: int
    block_type: str
    text: str
    heading: str
    level: Optional[int]
    page: Optional[int]
    language: str
    markdown: str
    html: str
    asset_path: str
    alt_text: str
    caption: str
    ocr_text: str


class RagStore:
    def __init__(self, db_path: str) -> None:
        self.db_path = str(db_path)
        self._conn = sqlite3.connect(self.db_path)
        self._conn.row_factory = sqlite3.Row
        self._conn.execute("PRAGMA foreign_keys = ON")
        self._conn.execute("PRAGMA journal_mode = WAL")

    @classmethod
    def open(cls, db_path: str) -> "RagStore":
        return cls(db_path)

    def close(self) -> None:
        self._conn.close()

    def __enter__(self) -> "RagStore":
        return self

    def __exit__(self, exc_type, exc, tb) -> None:
        self.close()

    def search_candidate_chunks(
        self,
        terms: Sequence[str],
        scope_paths: Sequence[str],
        limit: int,
    ) -> List[RagChunkRecord]:
        cleaned_terms = [term.strip() for term in terms if term and term.strip()]
        if not cleaned_terms:
            return []

        where_clauses = []
        where_params: List[Any] = []
        score_terms = []
        score_params: List[Any] = []

        if scope_paths:
            scope_clause = []
            for scope in scope_paths:
                trimmed = scope.strip()
                if not trimmed:
                    continue
                scope_clause.append("d.path LIKE ? ESCAPE '\\'")
                where_params.append(f"{_escape_like(trimmed)}%")
            if scope_clause:
                where_clauses.append("(" + " OR ".join(scope_clause) + ")")

        match_clauses = []
        for term in cleaned_terms:
            pattern = f"%{_escape_like(term)}%"
            match_clauses.append(
                "("
                "d.file_name LIKE ? ESCAPE '\\' OR "
                "d.path LIKE ? ESCAPE '\\' OR "
                "c.heading LIKE ? ESCAPE '\\' OR "
                "c.snippet LIKE ? ESCAPE '\\' OR "
                "d.content LIKE ? ESCAPE '\\'"
                ")"
            )
            where_params.extend([pattern, pattern, pattern, pattern, pattern])
            score_terms.append(
                "("
                "CASE "
                "WHEN d.file_name LIKE ? ESCAPE '\\' THEN 3 "
                "WHEN c.heading LIKE ? ESCAPE '\\' THEN 3 "
                "WHEN d.path LIKE ? ESCAPE '\\' THEN 2 "
                "WHEN c.snippet LIKE ? ESCAPE '\\' THEN 1 "
                "WHEN d.content LIKE ? ESCAPE '\\' THEN 1 "
                "ELSE 0 END"
                ")"
            )
            score_params.extend([pattern, pattern, pattern, pattern, pattern])

        where_clauses.append("(" + " OR ".join(match_clauses) + ")")
        sql = (
            "SELECT "
            "c.id AS chunk_id, "
            "c.document_id, "
            "d.file_name, "
            "d.path, "
            "d.ext, "
            "c.heading, "
            "c.snippet, "
            "c.paragraph, "
            "c.page, "
            "d.modified, "
            "d.modified_at, "
            "c.score, "
            "c.block_indexes_json, "
            "(" + " + ".join(score_terms) + ") AS sql_score "
            "FROM chunks c "
            "JOIN documents d ON d.id = c.document_id "
        )
        if where_clauses:
            sql += "WHERE " + " AND ".join(where_clauses) + " "
        sql += "ORDER BY sql_score DESC, d.modified_at DESC, c.rowid ASC LIMIT ?"
        # SQLite 按 SQL 文本里的占位符顺序绑定参数，`sql_score` 里的 CASE 占位符
        # 出现在 WHERE 条件之前，所以这里必须先绑定 score_params，再绑定 where_params。
        params = score_params + where_params + [int(limit)]

        rows = self._conn.execute(sql, params).fetchall()
        return [
            RagChunkRecord(
                chunk_id=row["chunk_id"],
                document_id=row["document_id"],
                file_name=row["file_name"],
                path=row["path"],
                ext=row["ext"],
                heading=row["heading"],
                snippet=row["snippet"],
                paragraph=row["paragraph"],
                page=row["page"],
                modified=row["modified"],
                modified_at=row["modified_at"],
                score=float(row["score"] if row["score"] is not None else row["sql_score"]),
                block_indexes_json=row["block_indexes_json"] or "[]",
            )
            for row in rows
        ]

    def list_document_chunks(self, path: str) -> List[RagChunkRecord]:
        row = self._conn.execute(
            "SELECT id FROM documents WHERE path = ? LIMIT 1",
            (path,),
        ).fetchone()
        if row is None:
            return []

        rows = self._conn.execute(
            """
            SELECT
                c.id AS chunk_id,
                c.document_id,
                d.file_name,
                d.path,
                d.ext,
                c.heading,
                c.snippet,
                c.paragraph,
                c.page,
                d.modified,
                d.modified_at,
                c.score,
                c.block_indexes_json
            FROM chunks c
            JOIN documents d ON d.id = c.document_id
            WHERE c.document_id = ?
            ORDER BY c.rowid
            """,
            (row["id"],),
        ).fetchall()
        return [
            RagChunkRecord(
                chunk_id=item["chunk_id"],
                document_id=item["document_id"],
                file_name=item["file_name"],
                path=item["path"],
                ext=item["ext"],
                heading=item["heading"],
                snippet=item["snippet"],
                paragraph=item["paragraph"],
                page=item["page"],
                modified=item["modified"],
                modified_at=item["modified_at"],
                score=float(item["score"]),
                block_indexes_json=item["block_indexes_json"] or "[]",
            )
            for item in rows
        ]

    def fetch_preview_blocks_for_document_ids(
        self,
        document_ids: Sequence[str],
    ) -> Dict[str, Dict[int, RagBlockRecord]]:
        cleaned_ids = [doc_id for doc_id in document_ids if doc_id]
        if not cleaned_ids:
            return {}

        placeholders = ",".join("?" for _ in cleaned_ids)
        rows = self._conn.execute(
            f"""
            SELECT
                document_id,
                block_index,
                block_type,
                text,
                heading,
                level,
                page,
                language,
                markdown,
                html,
                asset_path,
                alt_text,
                caption,
                ocr_text
            FROM document_blocks
            WHERE document_id IN ({placeholders})
            ORDER BY document_id, block_index
            """,
            cleaned_ids,
        ).fetchall()

        grouped: Dict[str, Dict[int, RagBlockRecord]] = {}
        for row in rows:
            grouped.setdefault(row["document_id"], {})[int(row["block_index"])] = RagBlockRecord(
                document_id=row["document_id"],
                block_index=int(row["block_index"]),
                block_type=row["block_type"],
                text=row["text"],
                heading=row["heading"],
                level=row["level"],
                page=row["page"],
                language=row["language"] or "",
                markdown=row["markdown"] or "",
                html=row["html"] or "",
                asset_path=row["asset_path"] or "",
                alt_text=row["alt_text"] or "",
                caption=row["caption"] or "",
                ocr_text=row["ocr_text"] or "",
            )
        return grouped

    def fetch_preview_blocks_for_chunks(
        self,
        chunk_records: Sequence[RagChunkRecord],
    ) -> Dict[str, List[Dict[str, Any]]]:
        document_ids = [record.document_id for record in chunk_records]
        blocks_by_document = self.fetch_preview_blocks_for_document_ids(document_ids)
        result: Dict[str, List[Dict[str, Any]]] = {}

        for record in chunk_records:
            try:
                block_indexes = json.loads(record.block_indexes_json or "[]")
            except Exception:  # noqa: BLE001
                block_indexes = []

            document_blocks = blocks_by_document.get(record.document_id, {})
            preview_blocks: List[Dict[str, Any]] = []
            for block_index in block_indexes:
                block = document_blocks.get(int(block_index))
                if block is None:
                    continue
                preview_blocks.append(
                    {
                        "block_index": block.block_index,
                        "block_type": block.block_type,
                        "text": block.text,
                        "heading": block.heading,
                        "level": block.level,
                        "page": block.page,
                        "language": block.language,
                        "markdown": block.markdown,
                        "html": block.html,
                        "asset_path": block.asset_path,
                        "alt_text": block.alt_text,
                        "caption": block.caption,
                        "ocr_text": block.ocr_text,
                    }
                )
            result[record.chunk_id] = preview_blocks

        return result
