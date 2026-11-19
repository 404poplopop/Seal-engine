#!/usr/bin/env python3
"""Pack an unsigned portable editor folder. No installer and no signature."""

import os
import struct
import subprocess
import sys
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SIZES = (16, 24, 32, 48, 64, 128, 256)


def chunk(tag, data):
    return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)


def png(size, rgb):
    raw = b"".join(b"\x00" + bytes(rgb) * size for _ in range(size))
    return b"".join(
        [
            b"\x89PNG\r\n\x1a\n",
            chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 2, 0, 0, 0)),
            chunk(b"IDAT", zlib.compress(raw, 9)),
            chunk(b"IEND", b""),
        ]
    )


def write_icons(folder):
    icons = folder / "icons"
    icons.mkdir(parents=True, exist_ok=True)
    # Seal blue, with a lighter corner so each size is a distinct file.
    for size in SIZES:
        (icons / f"seal-{size}.png").write_bytes(png(size, (28, 96, 168)))


def binary_path():
    override = os.environ.get("SEAL_PORTABLE_BIN")
    if override:
        return Path(override)
    name = "seal-editor.exe" if os.name == "nt" else "seal-editor"
    return ROOT / "target" / "release" / name


def main():
    dest = ROOT / "dist" / "SealPortable"
    if "--skip-build" not in sys.argv and not os.environ.get("SEAL_PORTABLE_BIN"):
        subprocess.check_call(["cargo", "build", "--release", "-p", "seal-editor", "--offline"], cwd=ROOT)
    src = binary_path()
    if not src.is_file():
        print(f"missing editor binary: {src}", file=sys.stderr)
        return 1
    dest.mkdir(parents=True, exist_ok=True)
    target = dest / src.name
    target.write_bytes(src.read_bytes())
    target.chmod(src.stat().st_mode)
    (dest / "portable.flag").write_text("portable\n")
    for name in ("LICENSE.md", "THIRD_PARTY_NOTICES.md"):
        source = ROOT / name
        if source.is_file():
            (dest / name).write_bytes(source.read_bytes())
    readme = dest / "README.txt"
    readme.write_text(
        "Seal editor portable folder.\n"
        "Unsigned. No installer. State is stored in user/ next to this program\n"
        "because portable.flag is present.\n"
        "Run seal-editor in this folder.\n"
    )
    write_icons(dest)
    print(dest)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
