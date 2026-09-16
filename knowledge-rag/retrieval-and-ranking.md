# Retrieval, Hybrid Fusion & Ranking Engine

> **Document Focus:** Dense vector similarity, BM25 lexical ranking, SQLite FTS5 fast-path dispatch, Reciprocal Rank Fusion (RRF), Cross-Encoder reranking, and query caching.

---

## 1. The Hybrid Retrieval Philosophy

Keyword search and semantic vector search possess complementary strengths and reciprocal weaknesses:
* **Dense Vector Search:** Excels at semantic intent, conceptual synonyms, and fuzzy matching (e.g., matching *"graceful termination"* to *"panic teardown"*). Fails at exact identifier lookups, GUIDs, and specific error codes (e.g., matching `0x800706BA` or `RPC_S_SERVER_UNAVAILABLE`).
* **Sparse Lexical Search (BM25 & FTS5):** Excels at exact technical tokens, function names, CLSIDs, and symbol identifiers. Fails when the query uses synonyms or paraphrased concepts.

`knowledge-rag` executes **hybrid retrieval** across both paths, fusing the result sets using Reciprocal Rank Fusion (RRF) and filtering through a deep cross-encoder.

```mermaid
graph TD
    Query["User / Agent Query Text"] --> Cache{"In QueryCache? (TTL 300s)"}
    Cache -->|Hit| FastRet["Return Cached Results Immediately"]
    Cache -->|Miss| Dispatch{"Search Method Mode"}
    
    Dispatch -->|"hybrid" or "auto"| Parallel
    
    subgraph Parallel ["Concurrent Stage 1: Retrieval"]
        VectorBranch["ChromaDB Vector Query (Dense Cosine Similarity)"]
        LexicalBranch["BM25 Inverted Index (Okapi BM25 Scoring)"]
        FTS5Branch["SQLite FTS5 MATCH Query (Optional Fast-Path)"]
    end
    
    Parallel --> RRF["Stage 2: Reciprocal Rank Fusion (RRF)"]
    RRF --> Filter["Category & Threshold Filtering"]
    Filter --> Rerank{"Reranker Available?"}
    
    subgraph Stage3 ["Stage 3: Deep Relevance Scoring"]
        Rerank -->|Yes| CrossEncoder["Cross-Encoder ONNX (ms-marco-MiniLM-L-6-v2)"]
        Rerank -->|No (Fallback)| PassThrough["Retain RRF Order with Score Normalization"]
    end
    
    CrossEncoder --> TopK["Slice Top-K Results (max_results)"]
    PassThrough --> TopK
    TopK --> StoreCache["Store in QueryCache (LRU)"]
    StoreCache --> Final["Return Formatted Chunks & Metadata"]
```

---

## 2. Stage 1: Dense & Sparse Retrieval Engines

### 2.1 Dense Vector Retrieval (ChromaDB)
* **Embedding Transformation:** The incoming query is transformed into a 384-dimensional dense vector using `FastEmbedEmbeddings.embed_query(query)` with `config.query_prefix` applied.
* **Vector Index:** ChromaDB executes an HNSW (Hierarchical Navigable Small World) graph search over the SQLite vector collection using cosine similarity:
  $$\text{Cosine Similarity}(u, v) = \frac{u \cdot v}{\|u\|_2 \|v\|_2}$$
* **Candidate Size:** The dense path retrieves $N = \max(20, \text{max\_results} \times 2)$ candidate chunks.

### 2.2 Sparse Lexical Retrieval (BM25)
* **Tokenization:** Query text is lowercased and split into whitespace/punctuation tokens.
* **BM25 Algorithm:** Calculates Okapi BM25 relevance scores over the corpus chunk vocabulary:
  $$\text{Score}(D, Q) = \sum_{i=1}^{n} \text{IDF}(q_i) \cdot \frac{f(q_i, D) \cdot (k_1 + 1)}{f(q_i, D) + k_1 \cdot \left(1 - b + b \cdot \frac{|D|}{\text{avgdl}}\right)}$$
  * Default parameters: $k_1 = 1.5$, $b = 0.75$.

### 2.3 SQLite FTS5 Fast-Path (`fts5_index.py`)
When enabled (`config.fts5_enabled = true`), lexical searches bypass Python-side BM25 and execute directly against SQLite's native C FTS5 engine (`fts5_index.db`).
* **Tokenizer:** Pinned by ADR-005 to `unicode61 remove_diacritics 2 tokenchars '-_.'`, ensuring technical hyphenated terms (e.g. `opc-da-client`) and dotted names (`Matrikon.OPC.Simulation.1`) are indexed as single lexical tokens.
* **Metacharacter Sanitization:** To prevent syntax errors or operator injection, all incoming tokens are sanitized:
  ```python
  def _escape_fts5_query(query: str) -> str:
      tokens = _FTS5_TOKEN_SPLIT.split(query.strip())
      escaped = [f'"{t.replace(chr(34), chr(34)+chr(34))}"' for t in tokens if t]
      return " ".join(escaped)
  ```

---

## 3. Stage 2: Reciprocal Rank Fusion (RRF)

Directly blending raw cosine similarity scores with BM25 scores is fundamentally flawed because vector cosine similarities $[0, 1]$ and BM25 scores $[0, \infty)$ operate on incompatible probability distributions.

`knowledge-rag` employs **Reciprocal Rank Fusion (RRF)**, which evaluates positional rankings rather than raw uncalibrated scores.

### 3.1 RRF Mathematical Formula
For a given chunk document $d$ appearing across rankers $M = \{\text{Dense}, \text{BM25}\}$:

$$\text{RRF\_Score}(d) = \sum_{m \in M} w_m \cdot \frac{1}{k + r_m(d)}$$

Where:
* $r_m(d) \in [1, N]$ is the 1-based ordinal rank of chunk $d$ in the output of retrieval engine $m$.
* $k$ is the smoothing constant (default: $k = 60$). The smoothing constant dampens the penalty difference between top-ranked items (e.g. rank 1 vs rank 2 is less extreme than if $k = 0$).
* $w_m$ is the engine weighting parameter derived from `hybrid_alpha`:
  * $w_{\text{dense}} = \text{hybrid\_alpha}$ (default: `0.5`)
  * $w_{\text{sparse}} = 1.0 - \text{hybrid\_alpha}$ (default: `0.5`)

If a document appears in only one ranker, it receives zero points for the missing ranker.

---

## 4. Stage 3: Cross-Encoder Reranking

Bi-encoder models (like BGE-small) calculate query and document embeddings independently, limiting the model's ability to evaluate token-level interactions.

In Stage 3, the top RRF candidates (typically top 20) are passed to a **Cross-Encoder Reranker** (`CrossEncoderReranker`).

### 4.1 Cross-Attention Mechanics
* **Model:** `Xenova/ms-marco-MiniLM-L-6-v2` executed via ONNX.
* **Input Structure:** Constructs joint sentence pairs with cross-attention across all tokens:
  ```text
  [CLS] <User Query> [SEP] <Chunk Content> [SEP]
  ```
* **Score Output:** A scalar logit passed through a sigmoid activation function $\sigma(z) \in [0, 1]$ representing the probability that the chunk satisfies the query.

### 4.2 Graceful Degradation
If the Cross-Encoder model is not downloaded, or if inference fails (e.g. host memory exhaustion), the orchestrator catches the exception, logs:
```text
[WARN] Reranker unavailable, using RRF order: <error>
```
and returns the candidates in their RRF order with normalized scores.

---

## 5. Query Caching (LRU + TTL)

To eliminate redundant vector inference when identical or repeated queries occur during multi-turn agent pairing:
* **Cache Mechanism:** `QueryCache` implements an in-memory Least Recently Used (LRU) dictionary.
* **Capacity & TTL:** `max_size = 100` queries, `ttl_seconds = 300` (5 minutes).
* **Cache Key Computation:**
  ```python
  cache_key = hashlib.sha256(
      f"{query}:{max_results}:{category_filter}:{hybrid_alpha}:{search_method}".encode()
  ).hexdigest()
  ```
* **Invalidation:** Whenever an indexing event occurs (file added, updated, or reindexed), `orchestrator.query_cache.clear()` is called immediately to prevent serving stale context.
