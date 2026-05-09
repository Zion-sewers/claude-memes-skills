#!/usr/bin/env python3
# seed-meme-vectordb.py — Pull Imgflip's free /get_memes endpoint and seed a
# LanceDB table at ~/.claude/cache/lancedb/memes. Optional component used by
# startup.sh; safe to run multiple times (idempotent — replaces table).
#
# Usage:
#   python3 seed-meme-vectordb.py [--db-path PATH] [--table NAME]
#                                 [--model NAME] [--no-embed]
#
# Source: https://api.imgflip.com/get_memes  (no API key, ToS allows public reuse)
# Returns ~100 most-popular meme templates with id/name/url/box_count.

from __future__ import annotations

import argparse
import json
import os
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

IMGFLIP_URL = "https://api.imgflip.com/get_memes"
DEFAULT_DB = Path.home() / ".claude" / "cache" / "lancedb"
DEFAULT_TABLE = "meme_templates"
DEFAULT_MODEL = "sentence-transformers/all-MiniLM-L6-v2"
JSON_FALLBACK = Path.home() / ".claude" / "cache" / "memes" / "templates.json"


def fetch_imgflip(retries: int = 3, backoff: float = 1.5) -> list[dict]:
    last_err: Exception | None = None
    for attempt in range(retries):
        try:
            req = urllib.request.Request(
                IMGFLIP_URL,
                headers={"User-Agent": "claude-meme-hooks/1.0 (+local install)"},
            )
            with urllib.request.urlopen(req, timeout=15) as resp:
                payload = json.loads(resp.read().decode("utf-8"))
            if not payload.get("success"):
                raise RuntimeError(f"imgflip returned success=false: {payload}")
            return payload["data"]["memes"]
        except (urllib.error.URLError, TimeoutError, RuntimeError) as e:
            last_err = e
            time.sleep(backoff ** attempt)
    raise RuntimeError(f"imgflip fetch failed after {retries} attempts: {last_err}")


def to_records(memes: list[dict]) -> list[dict]:
    out = []
    for m in memes:
        name = m.get("name", "").strip()
        if not name:
            continue
        out.append(
            {
                "id": str(m.get("id", "")),
                "name": name,
                "url": m.get("url", ""),
                "width": int(m.get("width", 0)),
                "height": int(m.get("height", 0)),
                "box_count": int(m.get("box_count", 0)),
                # description is what we embed: the human-recognizable label
                "description": f"{name} meme template ({m.get('box_count', 0)} text boxes)",
            }
        )
    return out


def write_json_fallback(records: list[dict]) -> Path:
    JSON_FALLBACK.parent.mkdir(parents=True, exist_ok=True)
    JSON_FALLBACK.write_text(json.dumps(records, indent=2, ensure_ascii=False))
    return JSON_FALLBACK


def seed_lancedb(records: list[dict], db_path: Path, table: str, model_name: str) -> None:
    try:
        import lancedb
        from sentence_transformers import SentenceTransformer
    except ImportError as e:
        raise SystemExit(
            f"lancedb/sentence-transformers not installed ({e}).\n"
            f"Install:  ~/.claude/assets/memes/.venv/bin/pip install lancedb sentence-transformers\n"
            f"Or rerun with --no-embed to skip the vectordb step."
        )

    db_path.mkdir(parents=True, exist_ok=True)
    db = lancedb.connect(str(db_path))

    print(f"loading embedding model: {model_name}", file=sys.stderr)
    model = SentenceTransformer(model_name)
    descriptions = [r["description"] for r in records]
    vectors = model.encode(descriptions, show_progress_bar=False, convert_to_numpy=True)

    rows = []
    for r, v in zip(records, vectors):
        rows.append({**r, "vector": v.tolist()})

    existing = db.list_tables() if hasattr(db, "list_tables") else db.table_names()
    if table in existing:
        db.drop_table(table)
    db.create_table(table, data=rows)
    print(f"seeded {len(rows)} rows into {db_path}/{table}.lance", file=sys.stderr)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--db-path", type=Path, default=DEFAULT_DB)
    ap.add_argument("--table", default=DEFAULT_TABLE)
    ap.add_argument("--model", default=DEFAULT_MODEL)
    ap.add_argument("--no-embed", action="store_true",
                    help="Write JSON only; skip LanceDB + sentence-transformers")
    args = ap.parse_args()

    print(f"fetching {IMGFLIP_URL}", file=sys.stderr)
    memes = fetch_imgflip()
    records = to_records(memes)
    if not records:
        print("no records returned from imgflip", file=sys.stderr)
        return 1

    fallback_path = write_json_fallback(records)
    print(f"wrote {len(records)} records to {fallback_path}", file=sys.stderr)

    if args.no_embed:
        return 0

    seed_lancedb(records, args.db_path, args.table, args.model)
    return 0


if __name__ == "__main__":
    sys.exit(main())
