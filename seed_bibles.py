import sqlite3, os, json, sys

DB = os.path.join(os.environ['APPDATA'], 'com.aletheia.production', 'aletheia.sqlite3')
RES = r'C:\Users\USER\OneDrive\Desktop\worship-production-interface\src-tauri\resources\bibles'

ABBREV = {
 "gn":"Genesis","ex":"Exodus","lv":"Leviticus","nm":"Numbers","dt":"Deuteronomy",
 "js":"Joshua","jud":"Judges","rt":"Ruth","1sm":"1 Samuel","2sm":"2 Samuel",
 "1kgs":"1 Kings","2kgs":"2 Kings","1ch":"1 Chronicles","2ch":"2 Chronicles",
 "ezr":"Ezra","ne":"Nehemiah","et":"Esther","job":"Job","ps":"Psalm","prv":"Proverbs",
 "ec":"Ecclesiastes","ss":"Song of Solomon","is":"Isaiah","jr":"Jeremiah","lm":"Lamentations",
 "ez":"Ezekiel","dn":"Daniel","ho":"Hosea","jl":"Joel","am":"Amos","ob":"Obadiah",
 "jn":"Jonah","mi":"Micah","na":"Nahum","hk":"Habakkuk","zp":"Zephaniah","hg":"Haggai",
 "zc":"Zechariah","ml":"Malachi","mt":"Matthew","mk":"Mark","lk":"Luke","jo":"John",
 "act":"Acts","rm":"Romans","1co":"1 Corinthians","2co":"2 Corinthians","gl":"Galatians",
 "eph":"Ephesians","ph":"Philippians","cl":"Colossians","1ts":"1 Thessalonians",
 "2ts":"2 Thessalonians","1tm":"1 Timothy","2tm":"2 Timothy","tt":"Titus","phm":"Philemon",
 "hb":"Hebrews","jm":"James","1pe":"1 Peter","2pe":"2 Peter","1jo":"1 John","2jo":"2 John",
 "3jo":"3 John","jd":"Jude","re":"Revelation",
}

def name_for(b):
    n = b.get('name')
    if n: return n
    a = (b.get('abbrev') or '').lower()
    return ABBREV.get(a, a or 'Unknown')

con = sqlite3.connect(DB, timeout=120, isolation_level=None)
cur = con.cursor()
cur.execute("PRAGMA busy_timeout=120000")
cur.execute("PRAGMA journal_mode=WAL")
try:
    cur.execute("PRAGMA wal_checkpoint(TRUNCATE)")
    print("checkpoint:", cur.fetchall(), flush=True)
except Exception as e:
    print("checkpoint warn:", e, flush=True)
for tid, fname in [('bbe','bbe-full.json')]:
    path = os.path.join(RES, fname)
    raw = open(path,'rb').read()
    if raw[:3] == b'\xef\xbb\xbf': raw = raw[3:]
    books = json.loads(raw.decode('utf-8'))
    cur.execute("BEGIN IMMEDIATE")
    n = 0
    for book in books:
        bn = name_for(book)
        for ci, ch in enumerate(book['chapters']):
            for vi, txt in enumerate(ch):
                cur.execute(
                    "INSERT INTO scripture_verses(translation_id,book,chapter,verse,text) "
                    "VALUES(?,?,?,?,?) "
                    "ON CONFLICT(translation_id,book,chapter,verse) DO UPDATE SET text=excluded.text",
                    (tid, bn, ci+1, vi+1, txt))
                cur.execute("DELETE FROM scripture_verses_fts WHERE translation_id=? AND book=? AND chapter=? AND verse=?",
                    (tid, bn, ci+1, vi+1))
                cur.execute("INSERT INTO scripture_verses_fts(translation_id,book,chapter,verse,text) VALUES(?,?,?,?,?)",
                    (tid, bn, ci+1, vi+1, txt))
                n += 1
    con.commit()
    print(f"{tid}: {n} verses imported", flush=True)

cur.execute("SELECT translation_id, COUNT(*) FROM scripture_verses GROUP BY translation_id")
for r in cur.fetchall(): print(r, flush=True)
cur.execute("SELECT text FROM scripture_verses WHERE translation_id='kjv' AND book='John' AND chapter=3 AND verse=16")
print('John 3:16 ->', cur.fetchone(), flush=True)
con.close()
