# Entity Knowledge Base

Aletheia uses two layers for Bible-character recall:

1. Automatic entity episode documents generated from the installed full Bible.
2. Optional curated tagged-entity datasets supplied by the operator or publisher.

The automatic layer extracts Bible names from every verse, groups them by
translation, book, chapter, and entity, then creates semantic documents such as:

- `story of Daniel`
- `what happened to Daniel`
- `scripture about Daniel`
- connected names in the same chapter
- the original chapter verse context

These generated documents are embedded into the offline FAISS vector index and
the Rust local TF-IDF warm index. This makes obscure-character searches work
without hand-writing one phrase list per person.

Automatic entity documents are capped at 8,000 per translation by default. The
cap prevents startup and FAISS builds from becoming unbounded while still
covering the full Bible with ranked episode summaries.

## Optional Tagged Dataset Format

Use JSON, JSONL, or `{ "entities": [...] }`.

```json
[
  {
    "entity": "Mephibosheth",
    "reference": "2 Samuel 9:7",
    "aliases": [
      "David showed kindness to Mephibosheth",
      "Jonathan's son who was lame in his feet"
    ],
    "keywords": [
      "covenant mercy",
      "restoration",
      "king's table"
    ],
    "summary": "David honors his covenant with Jonathan by restoring Saul's land to Mephibosheth."
  }
]
```

Accepted field aliases:

- `entity`, `name`, or `person`
- `reference` or `ref`
- `aliases`
- `keywords` or `themes`
- `summary`, `text`, or `description`

## Build Commands

Default build includes automatic entity episode documents:

```bash
npm run vector:build -- --force --translations kjv
```

Build with a different cap:

```bash
npm run vector:build -- --force --translations kjv --max-entity-docs 4000
```

Build with a curated tagged entity dataset:

```bash
npm run vector:build -- --force --translations kjv --entity-dataset C:\path\to\bible-entities.json
```

Environment-variable equivalent:

```bash
set ALETHEIA_ENTITY_DATASET=C:\path\to\bible-entities.json
npm run vector:build -- --force --translations kjv
```

Disable automatic generated entity documents only for debugging:

```bash
npm run vector:build -- --force --translations kjv --no-entity-docs
```
