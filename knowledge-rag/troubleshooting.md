# Knowledge-RAG Troubleshooting & Diagnostic Runbook

> **Document Focus:** Real-world failure modes, root-cause analyses, exact diagnostic commands, and permanent remediation playbooks for operators and developers.

---

## Quick Diagnostic Checklist

| Symptom | Primary Suspect | Fast Remediation Command |
|:---|:---|:---|
| `ValueError: setting an array element with a sequence` | Tokenizer Fixed Padding vs Truncation | Set `"strategy": "BatchLongest"` in `tokenizer.json` |
| `AlreadyRunningError` or exit `75` / `0xffffffff` | Stale `knowledge-rag.lock` file | Check PID, delete `<data_dir>/knowledge-rag.lock` |
| MCP client reports parse error or closes stream | Stray `print()` writing to `stdout` | Redirect `sys.stdout = sys.stderr` at startup |
| `sqlite3.DatabaseError: file is not a database` | ChromaDB disk write corruption | Trigger `_safe_get_collection()` wipe or rebuild |
| Background reindex never fires on new files | Watcher debounce starvation | Verify accumulate-mode debounce logic |
| `CUDAExecutionProvider not in onnxruntime providers` | Missing NVIDIA CUDA/cuDNN DLLs | Normal CPU fallback or install `onnxruntime-gpu` |

---

## 1. Issue: Inhomogeneous Array Shape in FastEmbed

### Symptoms
During `reindex_documents()` or `add_document()`, the process reports massive error rates (e.g., 27 errors out of 38 files), logging:
```text
[ERROR] Embedding generation FAILED: setting an array element with a sequence. 
The requested array has an inhomogeneous shape after 1 dimensions. 
The detected shape was (N,) + inhomogeneous part.
```

### Root Cause Analysis
1. FastEmbed's BGE ONNX snapshot ships with `tokenizer.json` configured with:
   ```json
   "padding": { "strategy": { "Fixed": 128 } }
   ```
2. FastEmbed's model loader overrides `truncation` to `512`, but skips updating `padding` because `tokenizer.padding` is already truthy.
3. In any batch containing a mix of short chunks (< 128 tokens) and long chunks (> 128 tokens, such as Markdown files with code snippets), the short chunks pad to length 128 while the long chunks stay unpadded.
4. When `onnx_text_model.py` calls `input_ids = np.array([e.ids for e in encoded])`, NumPy fails to construct a rectangular 2D matrix from ragged sequence lists.

### Permanent Remediation
Locate the model's `tokenizer.json` in the cache directory:
* Standard Cache: `<KNOWLEDGE_RAG_DIR>/models_cache/bge-small-en-v1.5/tokenizer.json`
* Snapshot Cache: `<KNOWLEDGE_RAG_DIR>/models_cache/models--qdrant--bge-small-en-v1.5-onnx-q/snapshots/main/tokenizer.json`

Replace lines 9–14:
```json
// BEFORE (Buggy)
"padding": {
  "strategy": {
    "Fixed": 128
  },
  "direction": "Right"
}

// AFTER (Fixed)
"padding": {
  "strategy": "BatchLongest",
  "direction": "Right"
}
```
Alternatively, in Python code, invoke `model.model.tokenizer.enable_padding()` immediately after instantiating `TextEmbedding`.

---

## 2. Issue: Single-Instance Lock Collision (`AlreadyRunningError`)

### Symptoms
* MCP client shows: `server name knowledge-rag failed to load: exit status 0xffffffff` or exit status `75` (`EX_TEMPFAIL`).
* Starting the server in CLI prints:
  ```text
  [ERROR] Another knowledge-rag server instance already holds the lock.
  ```

### Root Cause Analysis
When `KNOWLEDGE_RAG_SINGLE_INSTANCE=1` is set, `knowledge-rag` writes its PID to:
`<KNOWLEDGE_RAG_DIR>/data/knowledge-rag.lock`
If the previous server crashed, was forcibly terminated via `taskkill /F` or Task Manager, or another IDE instance is running in the background, the lock file persists.

### Diagnostic & Resolution
In PowerShell:
```powershell
# 1. Check if the lock file exists
Test-Path "$env:USERPROFILE\.knowledge-rag\data\knowledge-rag.lock"

# 2. Inspect the PID inside the lock file
Get-Content "$env:USERPROFILE\.knowledge-rag\data\knowledge-rag.lock"

# 3. Check if the process is actually alive
Get-Process -Id <PID> -ErrorAction SilentlyContinue

# 4. If stale, safely remove the lock file
Remove-Item "$env:USERPROFILE\.knowledge-rag\data\knowledge-rag.lock" -Force
```

---

## 3. Issue: MCP JSON-RPC Stream Corruption (`stdout` Pollution)

### Symptoms
* LLM client (Claude Code, Antigravity, Cursor) abruptly terminates the session with:
  `MCP protocol error: unexpected non-JSON message received on stdout`.
* MCP tools fail to register or report `connection closed`.

### Root Cause Analysis
Standard MCP transports over `stdio` require `stdout` to contain **strictly valid JSON-RPC 2.0 frames**.
Libraries such as `fitz` (PyMuPDF), `onnxruntime`, `urllib3`, or stray `print()` calls in user code emit warnings or diagnostic text directly to file descriptor 1 (`stdout`).

### Permanent Remediation
In your entry point module (e.g., `mcp_server/__init__.py`), intercept `sys.stdout` before any third-party libraries are imported:

```python
import sys

# Save pristine descriptor for MCP transport
_original_stdout = sys.stdout

# Force all standard library and third-party prints to stderr
sys.stdout = sys.stderr

# In server.py main():
# Restore _original_stdout ONLY to the MCP stdio transport handler
```

---

## 4. Issue: ChromaDB SQLite Corruption & Auto-Recovery

### Symptoms
* Server fails to start with:
  `sqlite3.DatabaseError: file is not a database` or `disk I/O error`.
* Vectors return zero matches despite files being present.

### Root Cause Analysis
Non-graceful termination during a multi-threaded vector write can leave the SQLite Write-Ahead Log (`chroma.sqlite3-wal`) or HNSW index binary in an inconsistent state.

### Permanent Remediation
Implement defensive collection retrieval (`_safe_get_collection`):

```python
def _safe_get_collection(self):
    try:
        return self.chroma_client.get_or_create_collection(
            name=config.collection_name,
            embedding_function=self.embed_fn
        )
    except Exception as exc:
        print(f"[CRITICAL] ChromaDB corrupted ({exc}). Rebuilding collection...", file=sys.stderr)
        shutil.rmtree(config.chroma_dir, ignore_errors=True)
        config.chroma_dir.mkdir(parents=True, exist_ok=True)
        return self.chroma_client.create_collection(
            name=config.collection_name,
            embedding_function=self.embed_fn
        )
```

---

## 5. Issue: Background Watcher Debounce Starvation

### Symptoms
When a user copies a large directory (e.g. 500 files) into `documents/`, the file watcher never triggers the reindexing run, or CPU spikes continuously without completing indexing.

### Root Cause Analysis
Naive debounce timers reset the timer on *every* file event:
```python
# BROKEN: Sliding timer reset
def on_modified(self, event):
    if self._timer:
        self._timer.cancel()
    self._timer = threading.Timer(10.0, self._do_reindex)
    self._timer.start()
```
If 500 files are copied sequentially over 30 seconds, the timer keeps resetting, starving the reindex execution.

### Permanent Remediation (Accumulate-Mode Debounce)
`knowledge-rag` uses **accumulate-mode debounce**: the timer is started on the *first* file event and is allowed to run to completion regardless of subsequent incoming events during that window:

```python
# FIXED: Accumulate-mode debounce
def _schedule_reindex(self, path: str):
    with self._lock:
        self._pending_paths.add(path)
        if self._timer is None or not self._timer.is_alive():
            self._timer = threading.Timer(self._debounce, self._do_reindex)
            self._timer.daemon = True
            self._timer.start()
```

---

## 6. Issue: Windows CUDA / GPU Acceleration Failure

### Symptoms
Server startup logs:
```text
GPU STATUS: UNAVAILABLE – running on CPU
Reason: CUDAExecutionProvider not in onnxruntime providers. Fix: pip install onnxruntime-gpu
```

### Root Cause Analysis
By default, `pip install onnxruntime` installs the CPU-only execution provider. Furthermore, Windows requires specific CUDA and cuDNN DLLs (`cublas64_12.dll`, `cudnn64_8.dll`) to be present in the system `PATH` or Python directory.

### Resolution Options
1. **Accept CPU Execution (Recommended for Desktop LLM pairing):**
   `bge-small-en-v1.5` is extremely lightweight (33M parameters). CPU embedding takes ~50ms per batch of 32 chunks on modern Intel/AMD processors. CPU execution avoids VRAM contention with local LLM models.
2. **Enable GPU Execution:**
   Run:
   ```powershell
   pip uninstall onnxruntime onnxruntime-gpu -y
   pip install onnxruntime-gpu --extra-index-url https://aiinfra.pkgs.visualstudio.com/PublicPackages/_packaging/onnxruntime-cuda-12/pypi/simple/
   pip install nvidia-cudnn-cu12 nvidia-cublas-cu12
   ```
