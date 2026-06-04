# DocMind Python 侧 RAG Graph 设计

## 1. 结论

DocMind 的问答能力要从“能召回、能回答”升级到“能稳定根据证据组织答案”，核心不在继续堆 prompt，而在把问答编排显式化为一个有状态的 RAG Graph。

推荐的目标形态是：

```text
retrieve -> pack_evidence -> draft_answer -> verify_answer -> repair_answer -> finalize
```

这套图可以用 LangGraph 实现，也可以在 Python sidecar 里用轻量状态机自己实现。关键不是框架本身，而是把“检索、组织证据、生成、校验、修复、拒答”拆成清晰节点。

对 DocMind 来说，RAG Graph 应完全放在 Python sidecar 侧，Rust/Tauri 只负责：

1. 接收前端提问。
2. 创建任务和 request id。
3. 传递 `db_path`、问题、范围和设置。
4. 消费 Python event 并驱动 UI。
5. 持久化最终答案、来源和 warning。

Rust 不应继续承载 query rewrite、证据重排、答案生成、引用校验这类 RAG 核心策略。

## 2. 为什么要做成 Graph

现在问答的主要问题不是“找不到相关片段”，而是“找到了片段，但答案没有按片段组织”。

Graph 设计比单函数链式调用更适合这个问题，原因有四个：

1. **可分支**：证据不足时走重试或拒答，不必硬把答案吐出来。
2. **可校验**：生成后能显式进入校验节点，检查引用覆盖和事实支撑。
3. **可追踪**：每个阶段都可以发 event，前端能看到检索、生成、校验进度。
4. **可扩展**：后续接 reranker、LLM judge、评测集，不需要重写主流程。

这也符合 DocMind 现有的 Python sidecar 任务模式：Rust 发起任务，Python 执行任务并持续输出 event，Rust 消费 event 并驱动 UI。

## 3. 设计目标

这版 RAG Graph 的目标不是追求最复杂，而是追求稳定、可解释、可维护。

完成后应满足：

1. 问答先做检索，再做证据组织，再做回答。
2. 答案正文不是“自由发挥”，而是基于 selected evidence 组织出来的。
3. 生成后有明确校验节点，不通过时可重试或拒答。
4. 流式输出和最终答案使用同一套协议，不出现“流式态和最终态不一致”。
5. Python 只回传最终选中的证据和最小必要上下文，不搬运大量未选中正文。

## 4. 推荐节点图

推荐的最小闭环如下：

```text
input
  -> retrieve
  -> rank
  -> pack_evidence
  -> draft_answer
  -> verify_answer
  -> repair_answer? -> draft_answer
  -> finalize
```

其中：

1. `retrieve`：多查询召回，得到候选 chunks。
2. `rank`：按“能否支撑答案”重排候选，而不是只按相似度排序。
3. `pack_evidence`：合并相邻 chunk、整理 source block、控制上下文预算。
4. `draft_answer`：基于证据生成答案，支持流式 `answer_delta`。
5. `verify_answer`：检查引用覆盖、事实句支撑、是否需要重试。
6. `repair_answer`：针对证据不足或引用缺失进行一次修复。
7. `finalize`：输出最终答案、sources、retrieval、warning、error。

如果用 LangGraph，这些就是一组节点和 conditional edges。  
如果不用 LangGraph，也应该以同样的节点职责来实现。

## 5. Graph State 设计

建议把状态定义成一个显式对象，不要在函数间隐式传全局变量。

### 5.1 输入状态

```json
{
  "request_id": "uuid",
  "db_path": "/path/to/docmind.sqlite",
  "question": "如何创建 Oracle 表空间",
  "recent_questions": ["上一轮问题1", "上一轮问题2"],
  "scope_paths": ["/Users/zhaoyang/Documents/803"],
  "session_id": "qa-session-id",
  "settings": {
    "provider": "openai_compatible",
    "base_url": "https://api.example.com/v1",
    "model": "gpt-4o-mini",
    "temperature": 0.2,
    "max_output_tokens": 6000,
    "context_chunk_limit": 8,
    "context_token_budget": 6000,
    "min_evidence_count": 2,
    "min_retrieval_score": 0.0
  }
}
```

### 5.2 中间状态

```json
{
  "rewritten_questions": [],
  "retrieval_candidates": [],
  "ranked_sources": [],
  "packed_evidence": [],
  "draft_answer": "",
  "answer_delta": "",
  "verification_result": null,
  "retry_count": 0,
  "warning": null,
  "error": null
}
```

### 5.3 输出状态

```json
{
  "ok": true,
  "state": "answered",
  "answer": "...",
  "sources": [],
  "retrieval": {},
  "warning": null,
  "error": null
}
```

状态设计原则：

1. Rust 只需要最小输入，不把大量 chunk 正文搬过来。
2. Python 自己通过 `db_path` 查询 SQLite。
3. 中间状态只保留候选 id、摘要和少量证据块。
4. 最终只回传 UI 需要的数据。

## 6. 节点职责

### 6.1 retrieve

职责：

1. 根据原问题和最近会话内容生成多个检索 query。
2. 结合 scope_paths 过滤目录范围。
3. 从 SQLite 中召回候选 chunks。

输入：

1. question。
2. recent_questions。
3. scope_paths。
4. settings。

输出：

1. 原始候选列表。
2. 每个候选的基础分、命中 query 数、文件名、标题路径、正文摘要。

建议事件：

```json
{
  "kind": "event",
  "event": "progress",
  "stage": "retrieve",
  "message": "正在检索候选证据",
  "percent": 20
}
```

### 6.2 rank

职责：

1. 对检索候选做问答专用重排。
2. 不只看相似度，还看“能否支撑答案”。

建议评分项：

1. query 命中数。
2. 标题路径命中。
3. heading 命中。
4. 正文关键字覆盖。
5. 同文档连续性。
6. 与会话强化词的贴合度。

输出：

1. ranked_sources。
2. selected_count。
3. 排序原因。

### 6.3 pack_evidence

职责：

1. 合并相邻 chunk。
2. 组装连续证据块。
3. 控制上下文 token 预算。
4. 生成 prompt 可直接消费的 evidence blocks。

这里不应把大量未选中正文一起发给模型。

建议每个 evidence block 包含：

1. source_id。
2. chunk_id。
3. file_name。
4. path。
5. title_path。
6. heading。
7. snippet。
8. 可选 preview_blocks 的轻量摘要，而不是完整结构化块。

### 6.4 draft_answer

职责：

1. 基于 packed_evidence 生成答案。
2. 支持真正流式输出 `answer_delta`。
3. 按要求输出 `[S1]` 之类的引用标注。

建议提示词原则：

1. 先基于证据组织答案，再输出结论。
2. 核心事实句必须带引用。
3. 辅助句可以少量不带引用，但不能吞掉核心证据。
4. 如果证据不足，明确说明不足，不要编造。

建议事件：

```json
{
  "kind": "event",
  "event": "answer_delta",
  "delta": "创建表空间需要先切换到 oracle 用户..."
}
```

### 6.5 verify_answer

职责：

1. 检查答案中的事实句是否有有效来源支撑。
2. 检查引用标注是否覆盖关键结论。
3. 判断是否进入 repair 或 finalize。

建议校验策略：

1. 事实句必须有有效引用。
2. 辅助句、过渡句、结构句可以没有引用。
3. 若核心事实无引用，进入 repair。
4. 若 repair 后仍不通过，进入拒答或降级输出。

这一步不建议只做“有没有 [Sx]”，而要做“引用是否覆盖答案核心事实”。

### 6.6 repair_answer

职责：

1. 对未通过校验的答案进行一次修复。
2. 重新生成时显式强调缺失点。

触发条件：

1. 引用缺失。
2. 事实覆盖不全。
3. 证据与结论不匹配。

修复后如果仍不通过，就不要再无限重试，应该明确拒答或降级提示。

### 6.7 finalize

职责：

1. 合并最终 answer。
2. 汇总 sources。
3. 汇总 retrieval 元信息。
4. 写入 warning / error。
5. 输出最终 response。

## 7. 事件协议建议

建议把 Python 侧事件分成三类：

1. `progress`：阶段进度。
2. `answer_delta`：答案正文增量。
3. `final`：最终结果。

### 7.1 progress

```json
{
  "kind": "event",
  "request_id": "uuid",
  "event": "progress",
  "stage": "rank",
  "message": "正在筛选证据",
  "percent": 45,
  "warning": null
}
```

### 7.2 answer_delta

```json
{
  "kind": "event",
  "request_id": "uuid",
  "event": "answer_delta",
  "delta": "WBS 是 Work Breakdown Structure..."
}
```

### 7.3 final

```json
{
  "kind": "response",
  "request_id": "uuid",
  "ok": true,
  "state": "answered",
  "answer": "WBS 是 ... [S1]",
  "warning": null,
  "error": null,
  "sources": [],
  "retrieval": {}
}
```

Rust 侧只需要：

1. 原样转发 progress。
2. 累计 answer_delta。
3. 收到 final 后一次性落库。

## 8. 与 DocMind 现有链路的边界

### 8.1 Rust 侧保留

1. UI 命令入口。
2. 任务 id / request id。
3. job 状态管理。
4. 会话持久化。
5. 事件转发。
6. fallback 控制。

### 8.2 Python 侧承担

1. query rewrite。
2. 召回。
3. 重排。
4. 证据组装。
5. 模型生成。
6. 引用校验。
7. 修复 / 重试 / 拒答。

### 8.3 不建议跨层搬运的内容

1. 大量 chunk 正文。
2. 全量 preview_blocks。
3. 所有候选未选中证据。

## 9. LangGraph 落地方式

如果使用 LangGraph，可以把上面这套节点直接映射为 state graph。

建议结构：

```text
START
  -> retrieve
  -> rank
  -> pack_evidence
  -> draft_answer
  -> verify_answer
      -> finalize
      -> repair_answer -> draft_answer
      -> refuse
END
```

优点：

1. 条件边清晰。
2. 节点职责单一。
3. 便于调试和日志观测。
4. 后续加评测集、人工干预、记忆节点都方便。

如果不引入 LangGraph，也建议保持同样的节点拆分，不要退回成一个大函数。

## 10. 推荐实施顺序

1. 先把 Python 侧现有 `rag_answer_stream` 按 Graph 结构拆清楚。
2. 先实现 `retrieve -> rank -> pack_evidence -> draft_answer -> verify_answer -> finalize` 的最小闭环。
3. 在 Python 侧保留一次 repair。
4. Rust 侧只做流式事件消费和持久化。
5. 再补本地评测集，验证真实准确率。
6. 最后再决定是否引入 LangGraph 作为正式编排库。

## 11. 完成标准

这套 RAG Graph 真正达标时，应满足：

1. 问答默认走 Python sidecar。
2. 检索、重排、生成、校验均可观测。
3. 答案正文可以流式输出。
4. 引用标注与答案结论可对应。
5. 证据不足时能 repair 或拒答，而不是硬编。
6. Rust 不再承载 RAG 策略细节。
7. 问答体验在“准确性”和“可追溯性”上明显提升。

