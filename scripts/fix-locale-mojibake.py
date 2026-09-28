#!/usr/bin/env python3
"""
Fix UTF-8 / CP1252 mojibake in moddin locale files.

Background
----------
The locale files in ``src/i18n/locales/*.ts`` were saved as UTF-8 by some
pipeline that read the original UTF-8 byte sequence as if it were
CP1252 / Windows-1252. As a result, every accented character and every
multi-byte glyph (ellipsis, em dash, sparkle, target emoji...) is now
displayed in the source as two, three, four or six mojibake characters.

Examples of mojibake this script repairs:

    VocÃª            -> Você
    InformaÃ§Ãµes    -> Informações
    AtualizaÃ§Ãµes   -> Atualizações
    Buscar jogoâ€¦   -> Buscar jogo…
    âœ¨ Auditar     -> ✨ Auditar
    ðŸŽ¯ Pedir       -> 🎯 Pedir
    âš ï¸ Diagnosticar -> ⚠️ Diagnosticar

How the fix works
-----------------
1. Read the file as UTF-8 (the source files are valid UTF-8; they just
   contain the wrong characters).
2. Walk the string left to right. At every position, try to consume a
   run of 2..6 characters whose code points, when re-encoded as CP1252
   bytes, decode back as a strictly shorter valid UTF-8 string. That
   condition is the precise definition of "this run is mojibake of some
   character that lost its UTF-8 encoding": a CP1252-style byte
   sequence that happens to be valid UTF-8.
3. Replace the run with the decoded string.
4. Anything that isn't mojibake (real Latin-1 letters used in
   Portuguese/Spanish, ASCII, or characters above CP1252's reach) is
   passed through unchanged.

Why it is safe
--------------
- Real Portuguese/Spanish characters (``ç``, ``ã``, ``ê``...) are all in
  the 0xC0..0xFF range where CP1252 == Unicode. Their bytes never
  combine with the next char's bytes to form a valid UTF-8 sequence, so
  the algorithm leaves them alone.
- The 0x80..0x9F range chars (the CP1252-only chars ``â``, ``Ã``,
  ``ð``, plus the four undefined bytes that Python exposes as the
  control chars U+0081, U+008D, U+008F, U+009D) only appear in this
  codebase as the start of mojibake runs. Plain Latin-1 text never uses
  them.
- The reduction check (``decoded_chars < run_chars``) is what keeps
  legitimate 2-byte sequences like ``ção`` untouched: their bytes
  decode to the same number of characters.
"""
from __future__ import annotations

import argparse
import sys
from pathlib import Path

BOM = "\ufeff"

# Full forward mapping: Unicode codepoint -> CP1252 byte.
# Built so every byte 0x00..0xFF is reachable, including the five
# undefined bytes (0x81, 0x8D, 0x8F, 0x90, 0x9D) which Python's
# "cp1252" codec refuses to encode. We expose them here as their own
# code points so the round-trip works on text the original pipeline
# produced.
CP1252_TO_UNICODE: dict[int, int] = {
    0x80: 0x20AC,  # €
    0x82: 0x201A,  # ‚
    0x83: 0x0192,  # ƒ
    0x84: 0x201E,  # „
    0x85: 0x2026,  # …
    0x86: 0x2020,  # †
    0x87: 0x2021,  # ‡
    0x88: 0x02C6,  # ˆ
    0x89: 0x2030,  # ‰
    0x8A: 0x0160,  # Š
    0x8B: 0x2039,  # ‹
    0x8C: 0x0152,  # Œ
    0x8E: 0x017D,  # Ž
    0x91: 0x2018,  # '
    0x92: 0x2019,  # '
    0x93: 0x201C,  # "
    0x94: 0x201D,  # "
    0x95: 0x2022,  # •
    0x96: 0x2013,  # –
    0x97: 0x2014,  # —
    0x98: 0x02DC,  # ˜
    0x99: 0x2122,  # ™
    0x9A: 0x0161,  # š
    0x9B: 0x203A,  # ›
    0x9C: 0x0153,  # œ
    0x9E: 0x017E,  # ž
    0x9F: 0x0178,  # Ÿ
}
# Invert the table: Unicode codepoint -> CP1252 byte.
UNICODE_TO_CP1252: dict[int, int] = {cp: byte for byte, cp in CP1252_TO_UNICODE.items()}
# Plus identity for 0xA0..0xFF (where CP1252 byte == Unicode codepoint).
for _cp in range(0xA0, 0x100):
    UNICODE_TO_CP1252.setdefault(_cp, _cp)
# Plus identity for 0x80..0x9F (the undefined bytes; control chars in
# Python's strict CP1252 codec but still present in our mojibake text).
for _cp in range(0x80, 0xA0):
    UNICODE_TO_CP1252.setdefault(_cp, _cp)
# ASCII is trivially identity (0x00..0x7F).
for _cp in range(0x00, 0x80):
    UNICODE_TO_CP1252.setdefault(_cp, _cp)


def cp1252_encode(text: str) -> bytes | None:
    """Encode ``text`` as CP1252 using our permissive table.

    Returns the byte string when every character has a mapping, or
    ``None`` if any character falls outside CP1252 (e.g. real emoji
    like ``⚠️`` whose variation selector has no byte form).
    """
    try:
        return bytes(UNICODE_TO_CP1252[ord(c)] for c in text)
    except KeyError:
        return None


def try_collapse(run: str) -> str | None:
    """Return the shorter UTF-8 decoding of ``run`` treated as CP1252
    bytes, or ``None`` if the run is not mojibake."""
    raw = cp1252_encode(run)
    if raw is None:
        return None
    try:
        decoded = raw.decode("utf-8")
    except UnicodeDecodeError:
        return None
    if len(decoded) >= len(run):
        return None
    return decoded


def fix_text(text: str) -> str:
    out: list[str] = []
    i = 0
    n = len(text)
    while i < n:
        best: str | None = None
        best_len = 0
        # Try longer runs first so e.g. a 6-char mojibake sequence is
        # collapsed as a unit instead of being split into 3-char pieces.
        for run_len in (6, 5, 4, 3, 2):
            if i + run_len > n:
                continue
            run = text[i:i + run_len]
            collapsed = try_collapse(run)
            if collapsed is None:
                continue
            best = collapsed
            best_len = run_len
            break
        if best is not None:
            out.append(best)
            i += best_len
        else:
            out.append(text[i])
            i += 1
    return "".join(out)


def fix_file(path: Path, write: bool = True) -> tuple[int, bool]:
    raw = path.read_text(encoding="utf-8-sig")
    fixed = fix_text(raw)
    changed = fixed != raw
    if changed and write:
        path.write_bytes(BOM.encode("utf-8") + fixed.encode("utf-8"))
    return len(raw.encode("utf-8")), changed


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "files",
        nargs="+",
        type=Path,
        help="One or more locale .ts files to fix.",
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="Only report which files would change; do not write anything.",
    )
    args = parser.parse_args()

    failures = 0
    for path in args.files:
        if not path.exists():
            print(f"[skip] missing: {path}")
            failures += 1
            continue
        try:
            original_size, changed = fix_file(path, write=not args.check)
        except RuntimeError as exc:
            print(f"[fail] {path}: {exc}")
            failures += 1
            continue

        if args.check:
            status = "would change" if changed else "clean"
        else:
            status = "rewrote" if changed else "clean"
        print(f"[{status}] {path} ({original_size} bytes)")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
