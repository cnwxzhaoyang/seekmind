"""
@author MorningSun
@CreatedDate 2026/06/18
@Description RAG 检索回归测试，覆盖离职同义召回与切片级匹配。
"""

from __future__ import annotations

import sqlite3
import tempfile
import unittest
from pathlib import Path

from seekmind_parser.rag.db import RagStore
from seekmind_parser.rag.models import RagSettings
from seekmind_parser.rag.retrieval import (
    collect_recall_candidates,
    select_recall_hits,
)

_TEST_TMP_DIRS: list[tempfile.TemporaryDirectory[str]] = []


def _create_test_db() -> Path:
    tmp_dir = tempfile.TemporaryDirectory()
    _TEST_TMP_DIRS.append(tmp_dir)
    db_path = Path(tmp_dir.name) / "rag.sqlite"
    conn = sqlite3.connect(db_path)
    conn.execute(
        """
        CREATE TABLE documents (
            id TEXT PRIMARY KEY,
            file_name TEXT NOT NULL,
            path TEXT NOT NULL,
            ext TEXT NOT NULL,
            content TEXT NOT NULL,
            modified TEXT NOT NULL,
            modified_at INTEGER NOT NULL
        )
        """
    )
    conn.execute(
        """
        CREATE TABLE chunks (
            id TEXT PRIMARY KEY,
            document_id TEXT NOT NULL,
            heading TEXT NOT NULL,
            snippet TEXT NOT NULL,
            paragraph INTEGER,
            page INTEGER,
            score REAL,
            block_indexes_json TEXT NOT NULL DEFAULT '[]'
        )
        """
    )
    conn.execute(
        """
        INSERT INTO documents
        (id, file_name, path, ext, content, modified, modified_at)
        VALUES
        ('doc-offboarding', '检索示例文本.md', '/tmp/hr/检索示例文本.md', 'md',
         '离职审批流程包括直属领导审批、人事确认、资产归还和账号注销。', '2026-06-18', 100),
        ('doc-expense', '费用制度.md', '/tmp/hr/费用制度.md', 'md',
         '费用报销流程需要提交发票、审批单和付款信息。', '2026-06-18', 150),
        ('doc-company', '公司国产化搭建实录.md', '/tmp/ops/公司国产化搭建实录.md', 'md',
         '离职这个词只存在于整篇文档内容里，但切片正文完全不相关。', '2026-06-18', 200)
        """
    )
    conn.execute(
        """
        INSERT INTO chunks
        (id, document_id, heading, snippet, paragraph, page, score, block_indexes_json)
        VALUES
        ('doc-offboarding:0', 'doc-offboarding', '离职审批流程',
         '离职审批流程包括直属领导审批、人事确认、资产归还和账号注销。', 1, NULL, 1.0, '[]'),
        ('doc-expense:0', 'doc-expense', '费用报销流程',
         '费用报销流程需要提交发票、审批单和付款信息。', 1, NULL, 1.0, '[]'),
        ('doc-company:0', 'doc-company', 'nginx 安装',
         '公司国产化环境中安装 nginx、jdk、redis 和 seata。', 1, NULL, 1.0, '[]')
        """
    )
    conn.commit()
    conn.close()
    return db_path


class RagRetrievalRegressionTest(unittest.TestCase):
    def test_offboarding_query_prefers_offboarding_chunk(self) -> None:
        db_path = _create_test_db()
        settings = RagSettings(context_chunk_limit=8, min_retrieval_score=0)
        with RagStore.open(str(db_path)) as store:
            candidates, _, _ = collect_recall_candidates(
                store,
                "离开公司要办理什么手续",
                [],
                settings,
                8,
                [],
            )
            selected = select_recall_hits(candidates, [], settings, 8)

        self.assertGreaterEqual(len(selected), 1)
        self.assertEqual(selected[0].chunk_id, "doc-offboarding:0")
        self.assertIn("离职审批流程", selected[0].snippet)
        self.assertIn("核心词", selected[0].recall_reason)
        self.assertIn("总分", selected[0].recall_reason)
        self.assertNotIn("doc-company:0", [hit.chunk_id for hit in selected])

    def test_custom_intent_synonym_rule_expands_query_terms(self) -> None:
        db_path = _create_test_db()
        settings = RagSettings(
            context_chunk_limit=8,
            min_retrieval_score=0,
            intent_synonym_rules_json="""
            [
              {
                "name": "expense",
                "markers": ["发票"],
                "recall_terms": ["费用报销", "报销流程", "发票", "付款"],
                "noise_terms": ["怎么"]
              }
            ]
            """,
        )
        with RagStore.open(str(db_path)) as store:
            candidates, _, _ = collect_recall_candidates(
                store,
                "发票怎么报",
                [],
                settings,
                8,
                [],
            )
            selected = select_recall_hits(candidates, [], settings, 8)

        self.assertGreaterEqual(len(selected), 1)
        self.assertEqual(selected[0].chunk_id, "doc-expense:0")
        self.assertIn("费用报销流程", selected[0].snippet)


if __name__ == "__main__":
    unittest.main()
