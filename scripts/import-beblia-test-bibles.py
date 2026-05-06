import os
import sqlite3
import sys
import urllib.request
import xml.etree.ElementTree as ET
from pathlib import Path

APP_ID = "com.aletheia.production"
RAW_BASE = "https://raw.githubusercontent.com/Beblia/Holy-Bible-XML-Format/master"

BOOKS = [
    "Genesis", "Exodus", "Leviticus", "Numbers", "Deuteronomy", "Joshua", "Judges", "Ruth",
    "1 Samuel", "2 Samuel", "1 Kings", "2 Kings", "1 Chronicles", "2 Chronicles", "Ezra", "Nehemiah",
    "Esther", "Job", "Psalm", "Proverbs", "Ecclesiastes", "Song of Solomon", "Isaiah", "Jeremiah",
    "Lamentations", "Ezekiel", "Daniel", "Hosea", "Joel", "Amos", "Obadiah", "Jonah", "Micah",
    "Nahum", "Habakkuk", "Zephaniah", "Haggai", "Zechariah", "Malachi", "Matthew", "Mark", "Luke",
    "John", "Acts", "Romans", "1 Corinthians", "2 Corinthians", "Galatians", "Ephesians",
    "Philippians", "Colossians", "1 Thessalonians", "2 Thessalonians", "1 Timothy", "2 Timothy",
    "Titus", "Philemon", "Hebrews", "James", "1 Peter", "2 Peter", "1 John", "2 John", "3 John",
    "Jude", "Revelation",
]

TARGETS = [
    ("nkjv", "New King James Version", "EnglishNKJBible.xml"),
    ("nlt", "New Living Translation", "EnglishNLTBible.xml"),
    ("gnb", "Good News Bible / Good News Translation", "EnglishGNTBible.xml"),
    ("esv", "English Standard Version", "EnglishESVBible.xml"),
    ("niv", "New International Version", "EnglishNIVBible.xml"),
    ("csb", "Christian Standard Bible", "EnglishCSBBible.xml"),
    ("amp", "Amplified Bible", "EnglishAmplifiedBible.xml"),
    ("tlb", "The Living Bible", "EnglishTLBible.xml"),
]


def appdata_root() -> Path:
    base = os.environ.get("APPDATA")
    if not base:
        raise RuntimeError("APPDATA is not set. Run this script from Windows Python.")
    return Path(base) / APP_ID


def download(target: Path, filename: str) -> None:
    if target.exists() and target.stat().st_size > 100_000:
        return
    url = f"{RAW_BASE}/{filename}"
    print(f"download {filename}", flush=True)
    with urllib.request.urlopen(url, timeout=120) as response:
        target.write_bytes(response.read())


def parse_beblia_xml(path: Path, translation_id: str):
    root = ET.parse(path).getroot()
    rows = []
    for book in root.iter("book"):
        raw_book_number = book.attrib.get("number", "")
        try:
            book_number = int(raw_book_number)
            book_name = BOOKS[book_number - 1]
        except Exception:
            book_name = book.attrib.get("name") or f"Book {raw_book_number or '?'}"
        for chapter in book.iter("chapter"):
            try:
                chapter_number = int(chapter.attrib["number"])
            except Exception:
                continue
            for verse in chapter.iter("verse"):
                try:
                    verse_number = int(verse.attrib["number"])
                except Exception:
                    continue
                text = "".join(verse.itertext()).strip()
                if text:
                    rows.append((translation_id, book_name, chapter_number, verse_number, text))
    return rows


def import_translation(con: sqlite3.Connection, translation_id: str, name: str, path: Path) -> int:
    rows = parse_beblia_xml(path, translation_id)
    if len(rows) < 20_000:
        raise RuntimeError(f"{translation_id}: expected a full Bible, parsed only {len(rows)} verses")

    cur = con.cursor()
    cur.execute("BEGIN IMMEDIATE")
    cur.execute(
        "INSERT INTO translations(id,name,language,license,offline_ready) VALUES(?,?,?,?,1) "
        "ON CONFLICT(id) DO UPDATE SET name=excluded.name, language=excluded.language, "
        "license=excluded.license, offline_ready=excluded.offline_ready",
        (translation_id, name, "English", "Beblia test import - verify publisher license before distribution"),
    )
    cur.execute("DELETE FROM scripture_verses WHERE translation_id=?", (translation_id,))
    cur.execute("DELETE FROM scripture_verses_fts WHERE translation_id=?", (translation_id,))
    cur.executemany(
        "INSERT INTO scripture_verses(translation_id,book,chapter,verse,text) VALUES(?,?,?,?,?)",
        rows,
    )
    cur.executemany(
        "INSERT INTO scripture_verses_fts(translation_id,book,chapter,verse,text) VALUES(?,?,?,?,?)",
        rows,
    )
    con.commit()
    return len(rows)


def main() -> int:
    root = appdata_root()
    db_path = root / "aletheia.sqlite3"
    if not db_path.exists():
        raise RuntimeError(f"Aletheia database not found: {db_path}")

    cache = root / "licensed-test-bibles" / "beblia"
    cache.mkdir(parents=True, exist_ok=True)

    con = sqlite3.connect(db_path, timeout=120, isolation_level=None)
    con.execute("PRAGMA busy_timeout=120000")
    con.execute("PRAGMA journal_mode=WAL")
    con.execute("PRAGMA wal_checkpoint(TRUNCATE)")

    for translation_id, name, filename in TARGETS:
        path = cache / filename
        download(path, filename)
        inserted = import_translation(con, translation_id, name, path)
        print(f"{translation_id}: {inserted:,} verses imported", flush=True)

    con.close()
    print("msg: not imported; Beblia does not provide an EnglishMSGBible.xml file", flush=True)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(1)
