# Custom Implementation Blueprint & Specification

> **Document Focus:** Step-by-step specification, interface contracts, and implementation blueprints for constructing an equivalent hybrid RAG system in Python or Rust.

---

## 1. Core Data Models & Contracts

Whether implemented in Python, Rust, or Go, an industrial-grade local RAG engine requires four canonical domain models:

### 1.1 Document Model
```python
@dataclass
class Document:
    id: str                        # 16-character SHA256 hex digest of resolved path
    source: Path                   # Absolute filesystem path
    category: str                  # Directory name or explicit tag (e.g. 'development', 'tars')
    format: str                    # File extension (e.g. '.md', '.rs')
    content: str                   # Full decoded raw string
    metadata: Dict[str, Any]       # Headers hierarchy, page count, file size, mtime
    keywords: List[str]            # Extracted technical keywords
    chunks: List[Chunk] = field(default_factory=list)
```

### 1.2 Chunk Model
```python
@dataclass
class Chunk:
    id: str                        # Composite ID: f"{doc_id}_{index:04d}"
    doc_id: str                    # Parent document reference ID
    index: int                     # 0-based ordinal index within the parent document
    content: str                   # Slice text (target length ~1000 characters)
    content_hash: str              # 20-character SHA256 digest of content (for dedup)
    metadata: Dict[str, Any]       # Section breadcrumb, line offsets
```

### 1.3 Search Result Contract
```python
@dataclass
class SearchResult:
    chunk_id: str
    doc_id: str
    source: str
    content: str
    score: float                   # Normalized relevance score in [0.0, 1.0]
    category: str
    metadata: Dict[str, Any]
```

---

## 2. Abstraction Interfaces (Traits)

To maintain decoupling, testability, and pluggability, implement the following four primary interfaces:

### 2.1 Embedding Engine
```python
class EmbeddingEngine(Protocol):
    def embed_passages(self, texts: List[str]) -> List[List[float]]:
        """Batch-embed document chunks with passage prefix (applies dynamic padding)."""
        ...

    def embed_query(self, query: str) -> List[float]:
        """Embed a single search query with query prefix."""
        ...
        
    @property
    def dimension(self) -> int:
        """Vector dimensionality (e.g., 384)."""
        ...
```

### 2.2 Vector Storage Backend
```python
class VectorStore(Protocol):
    def add_chunks(self, chunks: List[Chunk], embeddings: List[List[float]]) -> None:
        """Persist chunk vectors and metadata atomically."""
        ...

    def search_vector(self, query_vector: List[float], top_k: int) -> List[Tuple[str, float]]:
        """Return list of (chunk_id, cosine_distance) sorted ascending."""
        ...

    def delete_by_doc_id(self, doc_id: str) -> int:
        """Evict all chunk vectors belonging to the specified document ID."""
        ...
```

### 2.3 Lexical Search Backend
```python
class LexicalStore(Protocol):
    def add_chunks(self, chunks: List[Chunk]) -> None:
        """Tokenize and insert chunks into BM25 or SQLite FTS5 table."""
        ...

    def search_lexical(self, query_text: str, top_k: int) -> List[Tuple[str, float]]:
        """Return list of (chunk_id, bm25_score) sorted descending."""
        ...
```

### 2.4 Reranker Engine
```python
class RerankerEngine(Protocol):
    def rerank(self, query: str, candidate_chunks: List[Chunk]) -> List[Tuple[Chunk, float]]:
        """Calculate cross-attention relevance logits over (query, chunk) pairs."""
        ...
```

---

## 3. Minimal Standalone Implementation (Python)

Below is a self-contained, fully functioning reference implementation using `fastembed`, `chromadb`, and `rank-bm25`:

```python
"""
minimal_rag.py - Complete, self-contained hybrid RAG implementation
Dependencies: pip install fastembed chromadb rank-bm25
"""

import os
import hashlib
from pathlib import Path
from typing import List, Dict, Any, Tuple
from fastembed import TextEmbedding
import chromadb
from rank_bm25 import BM25Okapi

class MinimalHybridRAG:
    def __init__(self, data_dir: str = "./rag_data"):
        self.data_dir = Path(data_dir)
        self.data_dir.mkdir(parents=True, exist_ok=True)
        
        # 1. Initialize FastEmbed with dynamic batch padding
        self.embed_model = TextEmbedding(model_name="BAAI/bge-small-en-v1.5")
        if hasattr(self.embed_model, "model") and hasattr(self.embed_model.model, "tokenizer"):
            self.embed_model.model.tokenizer.enable_padding()
            
        # 2. Initialize ChromaDB (Dense Vector Store)
        self.chroma = chromadb.PersistentClient(path=str(self.data_dir / "chroma"))
        self.collection = self.chroma.get_or_create_collection(
            name="knowledge_base",
            metadata={"hnsw:space": "cosine"}
        )
        
        # 3. In-memory Lexical Index (BM25)
        self.bm25: BM25Okapi = None
        self.bm25_ids: List[str] = []
        self.corpus_chunks: Dict[str, Dict[str, Any]] = {}
        
    def chunk_text(self, text: str, chunk_size: int = 1000, overlap: int = 200) -> List[str]:
        chunks = []
        start = 0
        while start < len(text):
            end = start + chunk_size
            chunks.append(text[start:end])
            start += chunk_size - overlap
        return chunks

    def index_document(self, file_path: Path):
        file_path = file_path.resolve()
        content = file_path.read_text(encoding="utf-8", errors="ignore")
        doc_id = hashlib.sha256(str(file_path).encode()).hexdigest()[:16]
        
        text_chunks = self.chunk_text(content)
        chunk_ids = [f"{doc_id}_{i:04d}" for i in range(len(text_chunks))]
        metas = [{"source": str(file_path), "doc_id": doc_id, "index": i} for i in range(len(text_chunks))]
        
        # Embed with FastEmbed
        embeddings = [e.tolist() for e in self.embed_model.embed(text_chunks)]
        
        # Upsert into ChromaDB
        self.collection.upsert(ids=chunk_ids, documents=text_chunks, metadatas=metas, embeddings=embeddings)
        
        # Register in Lexical Store
        for cid, txt, m in zip(chunk_ids, text_chunks, metas):
            self.corpus_chunks[cid] = {"content": txt, "metadata": m}
            
        # Rebuild BM25
        self.bm25_ids = list(self.corpus_chunks.keys())
        tokenized_corpus = [self.corpus_chunks[cid]["content"].lower().split() for cid in self.bm25_ids]
        self.bm25 = BM25Okapi(tokenized_corpus)
        print(f"Indexed {file_path.name}: {len(text_chunks)} chunks.")

    def search(self, query: str, top_k: int = 5, hybrid_alpha: float = 0.5, rrf_k: int = 60) -> List[Dict[str, Any]]:
        # 1. Dense Search
        query_emb = list(self.embed_model.embed([query]))[0].tolist()
        chroma_res = self.collection.query(query_embeddings=[query_emb], n_results=top_k * 2)
        dense_ranked_ids = chroma_res["ids"][0]
        
        # 2. Sparse Lexical Search (BM25)
        sparse_ranked_ids = []
        if self.bm25:
            tokens = query.lower().split()
            scores = self.bm25.get_scores(tokens)
            sorted_indices = sorted(range(len(scores)), key=lambda i: scores[i], reverse=True)[:top_k * 2]
            sparse_ranked_ids = [self.bm25_ids[i] for i in sorted_indices if scores[i] > 0]
            
        # 3. Reciprocal Rank Fusion (RRF)
        rrf_scores: Dict[str, float] = {}
        for rank, cid in enumerate(dense_ranked_ids, start=1):
            rrf_scores[cid] = rrf_scores.get(cid, 0.0) + (hybrid_alpha * (1.0 / (rrf_k + rank)))
        for rank, cid in enumerate(sparse_ranked_ids, start=1):
            rrf_scores[cid] = rrf_scores.get(cid, 0.0) + ((1.0 - hybrid_alpha) * (1.0 / (rrf_k + rank)))
            
        sorted_results = sorted(rrf_scores.items(), key=lambda x: x[1], reverse=True)[:top_k]
        
        output = []
        for cid, score in sorted_results:
            chunk_data = self.corpus_chunks.get(cid, {})
            output.append({
                "chunk_id": cid,
                "score": score,
                "content": chunk_data.get("content", ""),
                "source": chunk_data.get("metadata", {}).get("source", "")
            })
        return output

if __name__ == "__main__":
    rag = MinimalHybridRAG()
    test_file = Path("test_knowledge.md")
    test_file.write_text("# Rust Safety\nIndustrial control systems require zero auto-retries on write commands.")
    rag.index_document(test_file)
    results = rag.search("Industrial control write commands")
    for r in results:
        print(f"Match (Score: {r['score']:.4f}): {r['content']}")
    test_file.unlink()
```

---

## 4. Rust Implementation Architecture (High-Performance Blueprint)

For production deployment requiring zero Python dependencies, single-binary distribution, and sub-10ms response latencies, implement in Rust:

### 4.1 Recommended Crate Ecosystem
* **Embedding Inference:** `fastembed` crate (native Rust FastEmbed port) or `ort` (ONNX Runtime bindings).
* **Storage & Vectors:** `sqlite-vec` extension or `qdrant-client` / embedded `libmdbx` + HNSW (`instant-distance` or `hora`).
* **Full-Text Lexical Search:** `rusqlite` with feature `bundled-full` enabling the native SQLite `FTS5` engine.
* **Filesystem Monitoring:** `notify` crate (cross-platform filesystem watcher wrapping Windows `ReadDirectoryChangesW`).
* **MCP Protocol Server:** Custom JSON-RPC over `tokio::io::stdin()` / `tokio::io::stdout()`.

### 4.2 Core Concurrency Architecture (Rust)
```mermaid
graph TD
    Tokio["Tokio Async Main Loop (stdio JSON-RPC)"] -->|MPSC unbounded channel| Worker["Worker Thread (Sync FFI / ONNX inference)"]
    Notify["notify crate (Background File Events)"] -->|Debounce Channel (10s)| IngestTask["Tokio Ingest Task"]
    IngestTask -->|Write Lock| SQLite["rusqlite (WAL mode + FTS5 + Vector blobs)"]
    Worker -->|Read Connection (concurrent)| SQLite
```
