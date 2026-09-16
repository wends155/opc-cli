# Knowledge-RAG Architecture & Reference Manual

> **System Purpose:** A self-contained, low-latency, hybrid Retrieval-Augmented Generation (RAG) system exposing Model Context Protocol (MCP) tools for persistent local knowledge retrieval, automatic filesystem change ingestion, and semantic search.

---

## 1. Executive Summary

`knowledge-rag` operates as a dedicated local knowledge subsystem. It bridges raw technical artifacts (markdown specifications, source code, whitepapers, schema definitions, and operational logs) with large language model (LLM) agents through standard Model Context Protocol (MCP) endpoints.

Unlike naive vector-only RAG implementations, `knowledge-rag` implements a **hybrid multi-stage retrieval architecture**:
1. **Dense Semantic Retrieval:** Vector embeddings powered by ONNX Runtime (`BAAI/bge-small-en-v1.5`, 384-dimensional dense vectors) persisted in an embedded ChromaDB SQLite database.
2. **Sparse Lexical Retrieval:** Inverted BM25 index combined with an SQLite FTS5 full-text search virtual table.
3. **Hybrid Rank Fusion:** Reciprocal Rank Fusion (RRF) combining dense cosine distance and sparse lexical scores.
4. **Cross-Encoder Reranking:** Deep cross-attention reranking via `Xenova/ms-marco-MiniLM-L-6-v2` to eliminate semantic drift before returning chunks to LLMs.
5. **Reactive Filesystem Ingestion:** A background `watchdog` daemon running accumulate-mode debounce logic that automatically ingests newly added, modified, or deleted documents in real time.

---

## 2. Technology Stack

| Layer | Component | Implementation / Technology | Purpose |
|:---|:---|:---|:---|
| **Protocol** | MCP Server | Python `mcp` SDK / JSON-RPC 2.0 | Exposes tools over `stdio` and `sse` (HTTP) |
| **Vector Database** | ChromaDB | `chromadb` (v1.4.0+) / SQLite backend | Persists vectors, chunk metadata, and document IDs |
| **Vector Inference** | FastEmbed | ONNX Runtime (`onnxruntime` / `onnxruntime-gpu`) | Generates 384-dim embeddings locally with CPU/CUDA fallback |
| **Lexical Engine** | BM25 + FTS5 | `rank-bm25` + SQLite `fts5` virtual table | Exact keyword, token, and symbol search |
| **Reranker** | Cross-Encoder | ONNX Runtime (`Xenova/ms-marco-MiniLM-L-6-v2`) | Cross-attention query-document relevance scoring |
| **Filesystem Watcher**| Watchdog | Python `watchdog` | Background directory monitoring with 10s debounce |
| **Document Parsers** | Multi-Format | Native Python, `pymupdf` (PDF), `pypdf` | Extracts text and structure across 18 file formats |
| **Security Layer** | Guardrails | Standard Library (`pathlib`, `hmac`, `hashlib`) | Path escape defense (CWE-22), injection neutralization (OWASP LLM01) |

---

## 3. Directory & Storage Layout

When configured with `KNOWLEDGE_RAG_DIR="<path>"` (default: `~/.knowledge-rag`), the system organizes state as follows:

```text
<KNOWLEDGE_RAG_DIR>/
├── documents/                      # Primary document corpus (monitored by watcher)
│   ├── development/                # Domain-specific documents (e.g., lessons.md)
│   ├── tars/                       # Standards & rules (e.g., coding-standard.md)
│   └── deps/                       # External dependency reference docs
├── data/
│   ├── chroma_db/                  # ChromaDB vector store
│   │   ├── chroma.sqlite3          # SQLite database storing collections & embeddings
│   │   └── <uuid>/                 # HNSW index binaries (vectors, link lists, headers)
│   ├── fts5_index.db               # SQLite FTS5 lexical index database (optional fast-path)
│   ├── index_metadata.json         # Document tracking (source, mtime, size, chunk count)
│   ├── reindex_checkpoint.json     # Resume checkpoint for interrupted reindex runs
│   └── knowledge-rag.lock          # Single-instance process lock (contains PID)
├── models_cache/                   # HuggingFace & FastEmbed model weights
│   ├── bge-small-en-v1.5/          # Local ONNX model weights and tokenizer config
│   └── models--qdrant--.../        # Cached FastEmbed model snapshots
└── logs/
    └── knowledge_rag.log           # Server trace and indexing diagnostics
```

---

## 4. Documentation Suite Index

This directory provides a complete blueprint for understanding, implementing, or troubleshooting `knowledge-rag`:

1. [Architecture & System Design](file:///c:/Users/WSALIGAN/code/opc-cli/knowledge-rag/architecture.md): Deep-dive into component topology, concurrency model, thread lifecycles, and SQLite WAL settings.
2. [Ingestion & Embedding Pipeline](file:///c:/Users/WSALIGAN/code/opc-cli/knowledge-rag/ingestion-and-embedding.md): Multi-format document parsing, chunking strategies, FastEmbed ONNX execution, and batching dynamics.
3. [Retrieval & Ranking Mechanics](file:///c:/Users/WSALIGAN/code/opc-cli/knowledge-rag/retrieval-and-ranking.md): Hybrid search equations, Reciprocal Rank Fusion (RRF), cross-encoder scoring, and query caching.
4. [Troubleshooting & Failure Modes](file:///c:/Users/WSALIGAN/code/opc-cli/knowledge-rag/troubleshooting.md): Comprehensive field guide covering real-world bugs (tokenizer padding mismatches, process lockouts, stdio pollution, and database corruption).
5. [Custom Implementation Blueprint](file:///c:/Users/WSALIGAN/code/opc-cli/knowledge-rag/implementation-guide.md): Step-by-step specification and sample interfaces to build a custom lightweight RAG engine in Python or Rust.
