# Offline Vector Knowledge Base

Aletheia supports an optional local FAISS + Sentence-Transformers semantic layer for paraphrase and partial-quote scripture retrieval.

## What It Does

- Reads every installed Bible translation from the local SQLite `scripture_verses` table.
- Generates `all-MiniLM-L6-v2` embeddings locally.
- Saves one FAISS cosine-similarity index per translation.
- Serves a localhost-only semantic search API.
- Lets the Rust desktop core call the semantic service with a short timeout and fall back to deterministic local search if the service is unavailable.

## Install

Use a virtual environment. On WSL/Linux, keep the environment on the Linux filesystem for speed:

```bash
python3 -m venv /tmp/aletheia-vector-venv
. /tmp/aletheia-vector-venv/bin/activate
python -m pip install -r scripts/vector-kb-requirements.txt
```

On Windows:

```cmd
py -3 -m venv .venv-vector
.venv-vector\Scripts\python.exe -m pip install -r scripts\vector-kb-requirements.txt
```

## Build Indexes

Build all installed translations:

```bash
. /tmp/aletheia-vector-venv/bin/activate
python scripts/vector_kb.py build \
  --database /mnt/c/Users/USER/AppData/Roaming/com.aletheia.production/aletheia.sqlite3 \
  --output /mnt/c/Users/USER/AppData/Roaming/com.aletheia.production/vector-kb \
  --batch-size 1024
```

The builder is resumable. If it stops halfway, rerun the same command and it skips completed translations.

CPU indexing can take hours for 300k+ verses. GPU acceleration can be enabled when a CUDA-capable PyTorch install is available:

```bash
ALETHEIA_VECTOR_DEVICE=cuda python scripts/vector_kb.py build --batch-size 2048
```

## Serve

```bash
. /tmp/aletheia-vector-venv/bin/activate
python scripts/vector_kb.py serve --kb-dir /mnt/c/Users/USER/AppData/Roaming/com.aletheia.production/vector-kb
```

The desktop app calls:

```text
http://127.0.0.1:47618/search
```

Override with:

```bash
ALETHEIA_VECTOR_KB_URL=http://127.0.0.1:47618/search
```

Disable the semantic bridge:

```bash
ALETHEIA_VECTOR_KB_DISABLED=1
```

## Runtime Safety

The Rust desktop core uses a 280ms timeout for vector search. If the vector service is slow, stopped, or missing indexes, scripture lookup still works through grammar parsing, learned phrases, phrase scoring, and SQLite FTS.
