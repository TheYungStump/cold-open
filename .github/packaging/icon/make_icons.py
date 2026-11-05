"""Makes the app's icons for every package (docs/specs/release.md §3):

    python3 .github/packaging/icon/make_icons.py [--from FOLDER]

Writes, beside this script:
- png/<size>.png at 16, 24, 32, 48, 64, 128, 256, 512 and 1024 pixels
  (the Linux hicolor icons, and the sources of the two below);
- coldopen.ico (Windows: the .exe and the installer), holding 16 to 256;
- coldopen.icns (macOS), holding 16 to 1024.

Without --from it draws the mark from its geometry: since 2026-11-05 the
"Opening Frame" stand-in the project owner chose (a C-shaped frame with a
red cursor bar on a navy tile; release.md §12), until final artwork
arrives. It also writes coldopen.svg (the icon) and mark.svg (the mark
alone, its frame in currentColor) for the website and the store graphics.
With --from, it takes <size>.png files at those sizes from FOLDER (exported
from the artwork) and only packs them. Pure Python, no image library.
"""

import os
import struct
import sys
import zlib

SIZES = [16, 24, 32, 48, 64, 128, 256, 512, 1024]
HERE = os.path.dirname(os.path.abspath(__file__))

TILE = (31, 42, 68)
PAGE = (247, 245, 239)
BAR = (214, 69, 65)


def rounded(x, y, x0, y0, x1, y1, r):
    """Whether (x, y) is inside the rectangle with corners rounded by r."""
    if not (x0 <= x <= x1 and y0 <= y <= y1):
        return False
    cx = min(max(x, x0 + r), x1 - r)
    cy = min(max(y, y0 + r), y1 - r)
    return (x - cx) ** 2 + (y - cy) ** 2 <= r * r


# The mark, "Opening Frame" (docs/specs/release.md §12; the project owner's
# stand-in of 2026-11-05, from design/brand-explorations/2026-11-05/
# 01-opening-frame.png): a C-shaped frame open to the right, with a red
# cursor bar standing in the opening. In 0..1 of the icon's square,
# measured from the concept board's app icon and evened out.
TILE_BOX = (0.04, 0.04, 0.96, 0.96)
TILE_RADIUS = 0.2
FRAME = (0.24, 0.24, 0.72, 0.76)  # the frame's outer edges: left, top, right, bottom
STROKE = 0.115  # the frame's thickness
CURSOR = (0.675, 0.385, 0.765, 0.615)  # the red bar: left, top, right, bottom


def in_box(u, v, box):
    return box[0] <= u <= box[2] and box[1] <= v <= box[3]


def in_frame(u, v):
    """Whether (u, v) is on the C-shaped frame."""
    left, top, right, bottom = FRAME
    if not in_box(u, v, FRAME):
        return False
    return u <= left + STROKE or v <= top + STROKE or v >= bottom - STROKE


def colour_at(u, v):
    """The icon's colour at (u, v) in 0..1, or None where transparent."""
    if not rounded(u, v, *TILE_BOX, TILE_RADIUS):
        return None
    if in_box(u, v, CURSOR):
        return BAR
    if in_frame(u, v):
        return PAGE
    return TILE


def svg(tile=True):
    """The mark as SVG, in a 1000-unit square: on its navy tile, or alone,
    with the frame in `currentColor` so a page can set it for its theme."""
    k = 1000

    def rect(box, fill, r=0):
        x0, y0, x1, y1 = (round(c * k) for c in box)
        rr = f' rx="{round(r * k)}"' if r else ""
        return f'<rect x="{x0}" y="{y0}" width="{x1 - x0}" height="{y1 - y0}"{rr} fill="{fill}"/>'

    def hexc(c):
        return "#%02x%02x%02x" % c

    left, top, right, bottom = FRAME
    frame_fill = hexc(PAGE) if tile else "currentColor"
    parts = []
    if tile:
        parts.append(rect(TILE_BOX, hexc(TILE), TILE_RADIUS))
    parts += [
        rect((left, top, right, top + STROKE), frame_fill),
        rect((left, bottom - STROKE, right, bottom), frame_fill),
        rect((left, top, left + STROKE, bottom), frame_fill),
        rect(CURSOR, hexc(BAR)),
    ]
    view = "0 0 1000 1000" if tile else "200 200 600 600"
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{view}" role="img" aria-label="Cold Open">'
        + "".join(parts)
        + "</svg>\n"
    )


def draw(size):
    """RGBA rows of the stand-in at `size`, 4x4 supersampled."""
    n = 4
    rows = []
    for y in range(size):
        row = bytearray()
        for x in range(size):
            acc = [0, 0, 0, 0]
            for sy in range(n):
                for sx in range(n):
                    c = colour_at((x + (sx + 0.5) / n) / size, (y + (sy + 0.5) / n) / size)
                    if c is not None:
                        acc[0] += c[0]
                        acc[1] += c[1]
                        acc[2] += c[2]
                        acc[3] += 255
            k = n * n
            a = acc[3] // k
            if a:
                cov = acc[3] / 255
                row += bytes((round(acc[0] / cov), round(acc[1] / cov), round(acc[2] / cov), a))
            else:
                row += b"\0\0\0\0"
        rows.append(bytes(row))
    return rows


def png(size, rows):
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

    raw = b"".join(b"\0" + r for r in rows)
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )


def ico(pngs):
    """An .ico of PNG entries (Windows Vista and later read these)."""
    sizes = [s for s in sorted(pngs) if s <= 256]
    out = struct.pack("<HHH", 0, 1, len(sizes))
    offset = 6 + 16 * len(sizes)
    data = b""
    for s in sizes:
        b = pngs[s]
        out += struct.pack("<BBBBHHII", s % 256, s % 256, 0, 0, 1, 32, len(b), offset + len(data))
        data += b
    return out + data


def icns(pngs):
    """An .icns of PNG entries, by the type codes macOS reads for each size."""
    kinds = [
        (b"icp4", 16),
        (b"icp5", 32),
        (b"icp6", 64),
        (b"ic07", 128),
        (b"ic08", 256),
        (b"ic09", 512),
        (b"ic10", 1024),
        (b"ic11", 32),
        (b"ic12", 64),
        (b"ic13", 256),
        (b"ic14", 512),
    ]
    body = b""
    for kind, s in kinds:
        b = pngs[s]
        body += kind + struct.pack(">I", 8 + len(b)) + b
    return b"icns" + struct.pack(">I", 8 + len(body)) + body


def main():
    source = None
    if len(sys.argv) == 3 and sys.argv[1] == "--from":
        source = sys.argv[2]
    elif len(sys.argv) != 1:
        raise SystemExit(__doc__)
    os.makedirs(os.path.join(HERE, "png"), exist_ok=True)
    pngs = {}
    for s in SIZES:
        if source:
            with open(os.path.join(source, f"{s}.png"), "rb") as f:
                pngs[s] = f.read()
        else:
            pngs[s] = png(s, draw(s))
        with open(os.path.join(HERE, "png", f"{s}.png"), "wb") as f:
            f.write(pngs[s])
    with open(os.path.join(HERE, "coldopen.ico"), "wb") as f:
        f.write(ico(pngs))
    with open(os.path.join(HERE, "coldopen.icns"), "wb") as f:
        f.write(icns(pngs))
    written = ["coldopen.ico", "coldopen.icns"]
    if not source:
        for name, tile in (("coldopen.svg", True), ("mark.svg", False)):
            with open(os.path.join(HERE, name), "w", encoding="utf-8", newline="\n") as f:
                f.write(svg(tile))
            written.append(name)
    print("wrote", ", ".join(f"{s}.png" for s in SIZES), ", ".join(written))


if __name__ == "__main__":
    main()
