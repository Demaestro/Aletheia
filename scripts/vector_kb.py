#!/usr/bin/env python3
"""Offline FAISS/Sentence-Transformers scripture vector knowledge base.

Builds one FAISS index per installed Bible translation from Aletheia's SQLite
database and serves a tiny localhost-only semantic search API for the desktop
app. The desktop app treats this as an optional accelerator: if this service is
not running, it falls back to deterministic grammar, phrase, and FTS search.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import sqlite3
import sys
import time
from dataclasses import asdict, dataclass
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any

MODEL_NAME = "sentence-transformers/all-MiniLM-L6-v2"
DEFAULT_PORT = 47618
MIN_SCORE = 0.55
DEFAULT_RUNTIME_MIN_SCORE = 0.55
DEFAULT_MAX_ENTITY_DOCS = 8_000


def canonical_book(book: str) -> str:
    normalized = " ".join(book.strip().split())
    folded = normalized.lower()
    if folded == "psalms":
        return "Psalm"
    if folded in {"song", "song of songs", "canticles"}:
        return "Song of Solomon"
    return normalized


@dataclass(frozen=True)
class VerseMeta:
    translation_id: str
    book: str
    chapter: int
    verse: int
    text: str
    doc_type: str = "verse"
    entity: str = ""

    @property
    def reference(self) -> str:
        return f"{self.book} {self.chapter}:{self.verse}"


def import_vector_deps() -> tuple[Any, Any, Any]:
    try:
        import faiss  # type: ignore
        import numpy as np  # type: ignore
        from sentence_transformers import SentenceTransformer  # type: ignore
    except ImportError as exc:
        raise SystemExit(
            "Missing vector KB dependencies. Run:\n"
            "  python -m pip install -r scripts/vector-kb-requirements.txt"
        ) from exc
    return faiss, np, SentenceTransformer


def default_database_path() -> Path:
    explicit = os.environ.get("ALETHEIA_DB")
    if explicit:
        return Path(explicit)

    if os.name == "nt":
        appdata = os.environ.get("APPDATA")
        if appdata:
            return Path(appdata) / "com.aletheia.production" / "aletheia.sqlite3"

    windows_user = os.environ.get("USER") or os.environ.get("USERNAME")
    if windows_user:
        candidate = (
            Path("/mnt/c/Users")
            / windows_user
            / "AppData/Roaming/com.aletheia.production/aletheia.sqlite3"
        )
        if candidate.exists():
            return candidate

    known = sorted(Path("/mnt/c/Users").glob("*/AppData/Roaming/com.aletheia.production/aletheia.sqlite3"))
    if known:
        return known[0]

    return Path.home() / ".local/share/com.aletheia.production/aletheia.sqlite3"


def default_output_dir(database_path: Path) -> Path:
    explicit = os.environ.get("ALETHEIA_VECTOR_KB_DIR")
    if explicit:
        return Path(explicit)
    return database_path.parent / "vector-kb"


def connect_readonly(database_path: Path) -> sqlite3.Connection:
    if not database_path.exists():
        raise SystemExit(f"Aletheia database not found: {database_path}")
    uri = f"file:{database_path.as_posix()}?mode=ro"
    return sqlite3.connect(uri, uri=True)


def installed_translations(connection: sqlite3.Connection) -> list[str]:
    rows = connection.execute(
        """
        SELECT translation_id, COUNT(*) AS verse_count
        FROM scripture_verses
        GROUP BY translation_id
        HAVING verse_count > 0
        ORDER BY translation_id
        """
    ).fetchall()
    return [row[0] for row in rows]


def read_verses(connection: sqlite3.Connection, translation_id: str) -> list[VerseMeta]:
    rows = connection.execute(
        """
        SELECT translation_id, book, chapter, verse, text
        FROM scripture_verses
        WHERE translation_id = ?
        ORDER BY rowid ASC
        """,
        (translation_id,),
    ).fetchall()
    deduped: dict[tuple[str, str, int, int], VerseMeta] = {}
    for row in rows:
        meta = VerseMeta(
            translation_id=row[0],
            book=canonical_book(row[1]),
            chapter=int(row[2]),
            verse=int(row[3]),
            text=row[4],
        )
        key = (meta.translation_id, meta.book, meta.chapter, meta.verse)
        existing = deduped.get(key)
        if existing is None or len(meta.text) < len(existing.text):
            deduped[key] = meta
    return list(deduped.values())


def embedding_text(meta: VerseMeta) -> str:
    if meta.doc_type == "entity":
        return f"{meta.reference}. Bible character {meta.entity}. Story of {meta.entity}. What happened to {meta.entity}. {meta.text}"
    return f"{meta.reference}. {meta.text}"


def translation_checksum(verses: list[VerseMeta]) -> str:
    digest = hashlib.sha256()
    for meta in verses:
        digest.update(meta.translation_id.encode("utf-8"))
        digest.update(b"\x1f")
        digest.update(meta.book.encode("utf-8"))
        digest.update(b"\x1f")
        digest.update(str(meta.chapter).encode("ascii"))
        digest.update(b"\x1f")
        digest.update(str(meta.verse).encode("ascii"))
        digest.update(b"\x1f")
        digest.update(meta.doc_type.encode("utf-8"))
        digest.update(b"\x1f")
        digest.update(meta.entity.encode("utf-8"))
        digest.update(b"\x1f")
        digest.update(meta.text.encode("utf-8"))
        digest.update(b"\n")
    return digest.hexdigest()


ENTITY_STOPWORDS = {
    "and", "but", "for", "the", "then", "therefore", "now", "when", "where",
    "who", "whom", "whose", "this", "that", "these", "those", "also", "behold",
    "lord", "god", "king", "chapter", "verse", "selah",
}


def is_entity_token(token: str) -> bool:
    token = token.strip("'\"`.,;:!?()[]{}")
    if len(token) < 3:
        return False
    folded = token.lower()
    if folded in ENTITY_STOPWORDS:
        return False
    return token[0].isupper() or (len(token) > 2 and token.isupper())


def extract_bible_entities(text: str) -> list[str]:
    entities: list[str] = []
    phrase: list[str] = []

    def push(value: str) -> None:
        cleaned = " ".join(value.strip().split())
        if len(cleaned) < 3 or cleaned.lower() in ENTITY_STOPWORDS:
            return
        if cleaned not in entities:
            entities.append(cleaned)

    def flush() -> None:
        nonlocal phrase
        if not phrase:
            return
        for item in phrase:
            push(item)
        if len(phrase) > 1:
            push(" ".join(phrase))
        phrase = []

    current = []
    for ch in text:
        if ch.isalnum() or ch in {"'", "-"}:
            current.append(ch)
            continue
        token = "".join(current).strip("'\"`-")
        current = []
        if token and is_entity_token(token):
            phrase.append(token)
        else:
            flush()
    token = "".join(current).strip("'\"`-")
    if token and is_entity_token(token):
        phrase.append(token)
    else:
        flush()
    flush()
    return entities


def synthesize_entity_documents(verses: list[VerseMeta], max_documents: int = DEFAULT_MAX_ENTITY_DOCS) -> list[VerseMeta]:
    drafts: dict[tuple[str, str, int, str], dict[str, Any]] = {}
    for meta in verses:
        entities = extract_bible_entities(meta.text)
        if not entities:
            continue
        for entity in entities:
            key = (meta.translation_id, meta.book, meta.chapter, entity)
            draft = drafts.setdefault(
                key,
                {
                    "translation_id": meta.translation_id,
                    "book": meta.book,
                    "chapter": meta.chapter,
                    "verse": meta.verse,
                    "entity": entity,
                    "co_entities": set(),
                    "texts": [],
                },
            )
            draft["verse"] = min(int(draft["verse"]), meta.verse)
            for other in entities:
                if other != entity:
                    draft["co_entities"].add(other)
            if sum(len(text) for text in draft["texts"]) < 8000:
                draft["texts"].append(meta.text)

    ranked_drafts: list[dict[str, Any]] = [
        draft
        for draft in drafts.values()
        if len(" ".join(draft["texts"]).split()) >= 8
    ]
    ranked_drafts.sort(
        key=lambda draft: (
            len(draft["co_entities"]),
            sum(len(text.split()) for text in draft["texts"]),
            1 if " " in str(draft["entity"]) else 0,
            str(draft["translation_id"]),
            str(draft["book"]),
            int(draft["chapter"]),
            int(draft["verse"]),
            str(draft["entity"]),
        ),
        reverse=True,
    )
    if max_documents > 0:
        ranked_drafts = ranked_drafts[:max_documents]

    documents: list[VerseMeta] = []
    for draft in ranked_drafts:
        joined_context = " ".join(draft["texts"])
        co_entities = " ".join(sorted(draft["co_entities"]))
        entity = str(draft["entity"])
        text = (
            f"{entity}. story of {entity}. what happened to {entity}. "
            f"scripture about {entity}. bible character {entity}. "
            f"connected people {co_entities}. chapter context {joined_context}"
        )
        documents.append(
            VerseMeta(
                translation_id=str(draft["translation_id"]),
                book=str(draft["book"]),
                chapter=int(draft["chapter"]),
                verse=int(draft["verse"]),
                text=text,
                doc_type="entity",
                entity=entity,
            )
        )
    return documents


def parse_reference(reference: str) -> tuple[str, int, int] | None:
    head = reference.strip().split("-", 1)[0].strip()
    if ":" not in head:
        return None
    book_chapter, verse_text = head.rsplit(":", 1)
    parts = book_chapter.strip().split()
    if len(parts) < 2:
        return None
    try:
        chapter = int(parts[-1])
        verse = int("".join(ch for ch in verse_text if ch.isdigit()))
    except ValueError:
        return None
    book = " ".join(parts[:-1])
    if not book or chapter < 1 or verse < 1:
        return None
    return canonical_book(book), chapter, verse


def read_tagged_entity_documents(path: Path, translation_id: str) -> list[VerseMeta]:
    if not path.exists():
        raise SystemExit(f"Tagged entity dataset not found: {path}")
    raw = path.read_text(encoding="utf-8")
    if path.suffix.lower() == ".jsonl":
        rows = [json.loads(line) for line in raw.splitlines() if line.strip()]
    else:
        parsed = json.loads(raw)
        rows = parsed.get("entities", parsed) if isinstance(parsed, dict) else parsed
    if not isinstance(rows, list):
        raise SystemExit("Tagged entity dataset must be a JSON array, JSONL file, or {\"entities\": [...]} object.")

    documents: list[VerseMeta] = []
    for row in rows:
        if not isinstance(row, dict):
            continue
        reference = str(row.get("reference") or row.get("ref") or "").strip()
        parsed_ref = parse_reference(reference)
        if parsed_ref is None:
            continue
        book, chapter, verse = parsed_ref
        entity = str(row.get("entity") or row.get("name") or row.get("person") or "").strip()
        aliases = row.get("aliases") or []
        keywords = row.get("keywords") or row.get("themes") or []
        summary = str(row.get("summary") or row.get("text") or row.get("description") or "").strip()
        if not entity:
            continue
        alias_text = " ".join(str(item) for item in aliases if str(item).strip())
        keyword_text = " ".join(str(item) for item in keywords if str(item).strip())
        text = (
            f"{entity}. story of {entity}. what happened to {entity}. "
            f"{alias_text} {keyword_text} {summary}"
        ).strip()
        documents.append(
            VerseMeta(
                translation_id=translation_id,
                book=book,
                chapter=chapter,
                verse=verse,
                text=text,
                doc_type="tagged-entity",
                entity=entity,
            )
        )
    return documents


def write_jsonl(path: Path, rows: list[dict[str, Any]]) -> None:
    with path.open("w", encoding="utf-8") as handle:
        for row in rows:
            handle.write(json.dumps(row, ensure_ascii=False, separators=(",", ":")))
            handle.write("\n")


def read_jsonl(path: Path) -> list[dict[str, Any]]:
    with path.open("r", encoding="utf-8") as handle:
        return [json.loads(line) for line in handle if line.strip()]


def build_indexes(args: argparse.Namespace) -> None:
    faiss, np, SentenceTransformer = import_vector_deps()
    database = Path(args.database).expanduser().resolve() if args.database else default_database_path()
    output_dir = Path(args.output).expanduser().resolve() if args.output else default_output_dir(database)
    output_dir.mkdir(parents=True, exist_ok=True)

    connection = connect_readonly(database)
    translations = args.translations or installed_translations(connection)
    if not translations:
        raise SystemExit("No Bible translations found in scripture_verses.")

    model = SentenceTransformer(args.model, device=args.device)
    manifest_path = output_dir / "manifest.json"
    if manifest_path.exists() and not args.force:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        if manifest.get("model") != args.model:
            raise SystemExit(
                f"Existing vector KB uses {manifest.get('model')}; rerun with --force to rebuild using {args.model}."
            )
        manifest["database"] = str(database)
    else:
        manifest = {
            "schema": 1,
            "model": args.model,
            "metric": "cosine",
            "normalization": "l2",
            "database": str(database),
            "builtAtMs": int(time.time() * 1000),
            "translations": {},
        }

    total = 0
    tagged_dataset = (
        Path(args.entity_dataset).expanduser().resolve()
        if args.entity_dataset
        else Path(os.environ["ALETHEIA_ENTITY_DATASET"]).expanduser().resolve()
        if os.environ.get("ALETHEIA_ENTITY_DATASET")
        else None
    )

    for translation_id in translations:
        verses = read_verses(connection, translation_id)
        if not verses:
            print(f"{translation_id}: skipped; no verses", flush=True)
            continue
        entity_docs = (
            synthesize_entity_documents(verses, max_documents=args.max_entity_docs)
            if not args.no_entity_docs
            else []
        )
        tagged_docs = (
            read_tagged_entity_documents(tagged_dataset, translation_id)
            if tagged_dataset is not None
            else []
        )
        documents = [*verses, *entity_docs, *tagged_docs]
        checksum = translation_checksum(documents)

        existing = manifest.get("translations", {}).get(translation_id)
        if existing and not args.force:
            index_path = output_dir / existing.get("index", "")
            meta_path = output_dir / existing.get("metadata", "")
            if (
                index_path.exists()
                and meta_path.exists()
                and int(existing.get("documents", existing.get("verses", 0))) == len(documents)
                and existing.get("checksum") == checksum
            ):
                print(f"{translation_id}: already indexed ({len(documents)} documents), skipping", flush=True)
                total += len(documents)
                continue

        texts = [embedding_text(meta) for meta in documents]
        started = time.perf_counter()
        embeddings = model.encode(
            texts,
            batch_size=args.batch_size,
            convert_to_numpy=True,
            normalize_embeddings=True,
            show_progress_bar=True,
        ).astype("float32")

        if embeddings.ndim != 2:
            raise RuntimeError(f"{translation_id}: invalid embedding shape {embeddings.shape}")

        index = faiss.IndexFlatIP(int(embeddings.shape[1]))
        index.add(np.ascontiguousarray(embeddings))

        index_path = output_dir / f"{translation_id}.faiss"
        meta_path = output_dir / f"{translation_id}.jsonl"
        faiss.write_index(index, str(index_path))
        write_jsonl(meta_path, [asdict(meta) | {"reference": meta.reference} for meta in documents])

        elapsed_ms = int((time.perf_counter() - started) * 1000)
        manifest["translations"][translation_id] = {
            "index": index_path.name,
            "metadata": meta_path.name,
            "verses": len(verses),
            "documents": len(documents),
            "entityDocuments": len(entity_docs),
            "taggedEntityDocuments": len(tagged_docs),
            "dimensions": int(embeddings.shape[1]),
            "buildMs": elapsed_ms,
            "checksum": checksum,
        }
        total += len(documents)
        manifest["totalVerses"] = sum(
            int(entry.get("verses", 0))
            for entry in manifest.get("translations", {}).values()
        )
        manifest["totalDocuments"] = sum(
            int(entry.get("documents", entry.get("verses", 0)))
            for entry in manifest.get("translations", {}).values()
        )
        manifest["builtAtMs"] = int(time.time() * 1000)
        manifest_path.write_text(
            json.dumps(manifest, indent=2, ensure_ascii=False),
            encoding="utf-8",
        )
        print(
            f"{translation_id}: indexed {len(documents)} documents "
            f"({len(verses)} verses, {len(entity_docs)} entity docs, {len(tagged_docs)} tagged docs) "
            f"in {elapsed_ms}ms",
            flush=True,
        )

    manifest["totalVerses"] = sum(
        int(entry.get("verses", 0))
        for entry in manifest.get("translations", {}).values()
    )
    manifest["totalDocuments"] = sum(
        int(entry.get("documents", entry.get("verses", 0)))
        for entry in manifest.get("translations", {}).values()
    )
    manifest["builtAtMs"] = int(time.time() * 1000)
    manifest_path.write_text(
        json.dumps(manifest, indent=2, ensure_ascii=False),
        encoding="utf-8",
    )
    print(f"Vector KB ready: {output_dir} ({total} documents)", flush=True)


class VectorKb:
    def __init__(self, kb_dir: Path, model_name: str, device: str):
        self.faiss, self.np, SentenceTransformer = import_vector_deps()
        self.kb_dir = kb_dir
        self.model = SentenceTransformer(model_name, device=device)
        self.manifest = json.loads((kb_dir / "manifest.json").read_text(encoding="utf-8"))
        self.indexes: dict[str, Any] = {}
        self.metadata: dict[str, list[dict[str, Any]]] = {}

        warm_translations = os.environ.get("ALETHEIA_VECTOR_WARM_TRANSLATIONS", "kjv")
        for translation in [item.strip().lower() for item in warm_translations.split(",")]:
            if translation:
                self.load_translation(translation)

    def load_translation(self, translation_id: str) -> bool:
        if translation_id in self.indexes:
            return True
        entry = self.manifest.get("translations", {}).get(translation_id)
        if not entry:
            return False
        self.indexes[translation_id] = self.faiss.read_index(str(self.kb_dir / entry["index"]))
        self.metadata[translation_id] = read_jsonl(self.kb_dir / entry["metadata"])
        return True

    def available_translations(self) -> list[str]:
        return sorted(self.manifest.get("translations", {}).keys())

    def search(
        self,
        query: str,
        translation_id: str,
        limit: int,
        min_score: float,
    ) -> list[dict[str, Any]]:
        query = query.strip()
        if len(query) < 4:
            return []

        translations = [translation_id]
        if translation_id in {"", "any", "*"}:
            translations = self.available_translations()

        vector = self.model.encode(
            [query],
            convert_to_numpy=True,
            normalize_embeddings=True,
            show_progress_bar=False,
        ).astype("float32")

        results_by_ref: dict[tuple[str, str, int, int], dict[str, Any]] = {}
        per_index_limit = max(limit, 8)
        for current_translation in translations:
            if not self.load_translation(current_translation):
                continue
            scores, indexes = self.indexes[current_translation].search(vector, per_index_limit)
            meta_rows = self.metadata[current_translation]
            for score, idx in zip(scores[0], indexes[0]):
                if idx < 0:
                    continue
                score_float = float(score)
                if score_float < min_score:
                    continue
                meta = dict(meta_rows[int(idx)])
                meta["book"] = canonical_book(str(meta.get("book", "")))
                meta["reference"] = f"{meta['book']} {meta['chapter']}:{meta['verse']}"
                meta["score"] = score_float
                meta["source"] = "Offline FAISS semantic search"
                key = (
                    str(meta.get("translation_id", current_translation)).lower(),
                    str(meta["book"]).lower(),
                    int(meta["chapter"]),
                    int(meta["verse"]),
                )
                existing = results_by_ref.get(key)
                if existing is None or score_float > float(existing.get("score", 0.0)):
                    results_by_ref[key] = meta

        results = list(results_by_ref.values())
        results.sort(key=lambda row: row["score"], reverse=True)
        return results[:limit]


def serve(args: argparse.Namespace) -> None:
    kb_dir = Path(args.kb_dir).expanduser().resolve() if args.kb_dir else default_output_dir(default_database_path())
    kb = VectorKb(kb_dir, args.model, args.device)

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, _format: str, *_args: Any) -> None:
            return

        def _send(self, status: int, payload: dict[str, Any]) -> None:
            body = json.dumps(payload, ensure_ascii=False).encode("utf-8")
            self.send_response(status)
            self.send_header("Content-Type", "application/json; charset=utf-8")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def do_GET(self) -> None:
            if self.path == "/health":
                self._send(200, {"ok": True, "translations": kb.available_translations(), "model": args.model})
                return
            self._send(404, {"error": "not found"})

        def do_POST(self) -> None:
            if self.path != "/search":
                self._send(404, {"error": "not found"})
                return
            try:
                length = min(int(self.headers.get("Content-Length", "0")), 64 * 1024)
                payload = json.loads(self.rfile.read(length).decode("utf-8"))
                started = time.perf_counter()
                results = kb.search(
                    str(payload.get("query", "")),
                    str(payload.get("translationId", payload.get("translation_id", "kjv"))).lower(),
                    int(payload.get("limit", 5)),
                    float(payload.get("minScore", payload.get("min_score", DEFAULT_RUNTIME_MIN_SCORE))),
                )
                self._send(
                    200,
                    {
                        "results": results,
                        "latencyMs": int((time.perf_counter() - started) * 1000),
                        "model": args.model,
                    },
                )
            except Exception as exc:  # noqa: BLE001 - local diagnostic API
                self._send(500, {"error": str(exc)})

    server = ThreadingHTTPServer((args.host, args.port), Handler)
    print(f"Vector KB serving {kb_dir} on http://{args.host}:{args.port}", flush=True)
    server.serve_forever()


def search_once(args: argparse.Namespace) -> None:
    kb_dir = Path(args.kb_dir).expanduser().resolve() if args.kb_dir else default_output_dir(default_database_path())
    kb = VectorKb(kb_dir, args.model, args.device)
    payload = {
        "results": kb.search(args.query, args.translation.lower(), args.limit, args.min_score),
        "model": args.model,
    }
    print(json.dumps(payload, ensure_ascii=False, indent=2))


def main() -> None:
    parser = argparse.ArgumentParser(description="Aletheia offline scripture vector knowledge base")
    sub = parser.add_subparsers(dest="command", required=True)

    build = sub.add_parser("build", help="Build FAISS indexes from the local SQLite Bible database")
    build.add_argument("--database", default=None)
    build.add_argument("--output", default=None)
    build.add_argument("--model", default=MODEL_NAME)
    build.add_argument("--device", default=os.environ.get("ALETHEIA_VECTOR_DEVICE", "cpu"))
    build.add_argument("--batch-size", type=int, default=96)
    build.add_argument("--translations", nargs="*", default=None)
    build.add_argument(
        "--entity-dataset",
        default=None,
        help="Optional JSON/JSONL tagged Bible entity dataset. Rows accept entity/name/person, reference/ref, aliases, keywords/themes, and summary/text/description.",
    )
    build.add_argument(
        "--no-entity-docs",
        action="store_true",
        help="Disable automatic entity episode documents extracted from the local Bible text.",
    )
    build.add_argument(
        "--max-entity-docs",
        type=int,
        default=DEFAULT_MAX_ENTITY_DOCS,
        help="Maximum automatic entity episode documents per translation. Use 0 for no cap.",
    )
    build.add_argument("--force", action="store_true")
    build.set_defaults(func=build_indexes)

    serve_parser = sub.add_parser("serve", help="Serve localhost semantic scripture search")
    serve_parser.add_argument("--kb-dir", default=None)
    serve_parser.add_argument("--model", default=MODEL_NAME)
    serve_parser.add_argument("--device", default=os.environ.get("ALETHEIA_VECTOR_DEVICE", "cpu"))
    serve_parser.add_argument("--host", default="127.0.0.1")
    serve_parser.add_argument("--port", type=int, default=DEFAULT_PORT)
    serve_parser.set_defaults(func=serve)

    search_parser = sub.add_parser("search", help="Run one semantic search")
    search_parser.add_argument("query")
    search_parser.add_argument("--kb-dir", default=None)
    search_parser.add_argument("--model", default=MODEL_NAME)
    search_parser.add_argument("--device", default=os.environ.get("ALETHEIA_VECTOR_DEVICE", "cpu"))
    search_parser.add_argument("--translation", default="kjv")
    search_parser.add_argument("--limit", type=int, default=5)
    search_parser.add_argument("--min-score", type=float, default=MIN_SCORE)
    search_parser.set_defaults(func=search_once)

    args = parser.parse_args()
    try:
        args.func(args)
    except KeyboardInterrupt:
        sys.exit(130)


if __name__ == "__main__":
    main()
