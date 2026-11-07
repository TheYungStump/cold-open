"""The MSIX package's logos (docs/specs/release.md §11.4), drawn by the
icon's own code (../icon/make_icons.py):

    python3 .github/packaging/msix/make_assets.py OUT_DIR

Square44x44Logo (the taskbar and Start's list), Square150x150Logo (the
tile), Wide310x150Logo (the wide tile, the icon centred on a transparent
field) and StoreLogo (50 pixels). Pure Python, as make_icons.py is.
"""

import importlib.util
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
spec = importlib.util.spec_from_file_location("make_icons", os.path.join(HERE, "..", "icon", "make_icons.py"))
icons = importlib.util.module_from_spec(spec)
spec.loader.exec_module(icons)


def wide(width, height):
    """The icon at the height, centred in a transparent width x height."""
    square = icons.draw(height)
    pad = (width - height) // 2
    rows = []
    for r in square:
        rows.append(b"\0\0\0\0" * pad + r + b"\0\0\0\0" * (width - height - pad))
    return rows


def png(width, height, rows):
    import struct
    import zlib

    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

    raw = b"".join(b"\0" + r for r in rows)
    return (b"\x89PNG\r\n\x1a\n"
            + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw, 9))
            + chunk(b"IEND", b""))


def main():
    if len(sys.argv) != 2:
        raise SystemExit(__doc__)
    out = sys.argv[1]
    os.makedirs(out, exist_ok=True)
    for name, size in [("Square44x44Logo", 44), ("Square150x150Logo", 150), ("StoreLogo", 50)]:
        with open(os.path.join(out, f"{name}.png"), "wb") as f:
            f.write(icons.png(size, icons.draw(size)))
    with open(os.path.join(out, "Wide310x150Logo.png"), "wb") as f:
        f.write(png(310, 150, wide(310, 150)))
    print("wrote", ", ".join(sorted(os.listdir(out))))


if __name__ == "__main__":
    main()
