"""Makes the app's icons for every package (docs/specs/release.md §3):

    python3 .github/packaging/icon/make_icons.py [--from FOLDER]

Writes, beside this script:
- png/<size>.png at 16, 24, 32, 48, 64, 128, 256, 512 and 1024 pixels
  (the Linux hicolor icons, and the sources of the two below);
- coldopen.ico (Windows: the .exe and the installer), holding 16 to 256;
- coldopen.icns (macOS), holding 32 to 1024 in its PNG slots.

Without --from it draws the mark from its geometry: the project owner's
brand, finalized 2026-11-06 (release.md §12): a rounded C, its open side a
separate arc cut off by two diagonal gaps, around three lines of a script,
in paper on a blue tile. It also writes coldopen.svg (the icon) and
mark.svg (the mark in its colors: blue C, dark arc and lines) for the
website and the store graphics.
With --from, it takes <size>.png files at those sizes from FOLDER (exported
from the artwork) and only packs them. Pure Python, no image library.
"""

import os
import struct
import sys
import zlib

SIZES = [16, 24, 32, 48, 64, 128, 256, 512, 1024]
HERE = os.path.dirname(os.path.abspath(__file__))

TILE = (12, 68, 124)  # #0C447C
PAGE = (247, 245, 239)  # #F7F5EF
INK = (35, 35, 38)  # #232326


def rounded(x, y, x0, y0, x1, y1, r):
    """Whether (x, y) is inside the rectangle with corners rounded by r."""
    if not (x0 <= x <= x1 and y0 <= y <= y1):
        return False
    cx = min(max(x, x0 + r), x1 - r)
    cy = min(max(y, y0 + r), y1 - r)
    return (x - cx) ** 2 + (y - cy) ** 2 <= r * r


# The mark, in its own 100-unit square (docs/site/static/mark.svg): the
# ring between two rounded rectangles, cut by two diagonal gaps along
# x + y = 92.6 and x - y = -7.4; the part beyond them is the arc; three
# lines inside. On the icon it sits at 15 + 0.7 * m in a 100-unit tile.
OUTER = (0, 0, 100, 100, 30)
INNER = (14, 13, 86, 87, 18)
GAP = 1.8  # half the gaps' width
LINES = [(34, 39.5, 67, 43.7), (39.5, 48, 70.5, 52.2), (34, 56.5, 61.5, 60.7)]
TILE_RADIUS = 24  # in the tile's 100 units
MARK_AT, MARK_SCALE = 15, 0.7


def in_ring(x, y):
    return rounded(x, y, *OUTER) and not rounded(x, y, *INNER)


def in_gap(x, y):
    return x > 60 and (abs(x + y - 92.6) < GAP * 2 ** 0.5 or abs(x - y + 7.4) < GAP * 2 ** 0.5)


def in_arc(x, y):
    return x + y > 92.6 and x - y > -7.4


def in_lines(x, y):
    return any(x0 <= x <= x1 and y0 <= y <= y1 for x0, y0, x1, y1 in LINES)


def colour_at(u, v):
    """The icon's colour at (u, v) in 0..1, or None where transparent:
    paper on a blue tile, as the brand's app icon."""
    x, y = u * 100, v * 100
    if not rounded(x, y, 0, 0, 100, 100, TILE_RADIUS):
        return None
    mx, my = (x - MARK_AT) / MARK_SCALE, (y - MARK_AT) / MARK_SCALE
    if (in_ring(mx, my) and not in_gap(mx, my)) or in_lines(mx, my):
        return PAGE
    return TILE


RING_PATH = ("M30 0H70A30 30 0 0 1 100 30V70A30 30 0 0 1 70 100H30A30 30 0 0 1 0 70V30A30 30 0 0 1 30 0Z"
             "M32 13H68A18 18 0 0 1 86 31V69A18 18 0 0 1 68 87H32A18 18 0 0 1 14 69V31A18 18 0 0 1 32 13Z")
GAPS = ('<mask id="co-gaps" maskUnits="userSpaceOnUse" x="-10" y="-10" width="120" height="120">'
        '<rect x="-10" y="-10" width="120" height="120" fill="#fff"/>'
        '<path d="M66 26.6 104 -11.4M66 73.4 104 111.4" stroke="#000" stroke-width="3.6"/></mask>')


def svg(tile=True):
    """The icon (paper on the blue tile), or the mark in its colors."""
    def hexc(c):
        return "#%02X%02X%02X" % c

    lines = "".join(f'<rect x="{x0}" y="{y0}" width="{round(x1 - x0, 1)}" height="{round(y1 - y0, 1)}"/>'
                    for x0, y0, x1, y1 in LINES)
    if tile:
        body = (f'<defs>{GAPS}</defs><rect width="100" height="100" rx="{TILE_RADIUS}" fill="{hexc(TILE)}"/>'
                f'<g transform="translate({MARK_AT} {MARK_AT}) scale({MARK_SCALE})" fill="{hexc(PAGE)}">'
                f'<path mask="url(#co-gaps)" fill-rule="evenodd" d="{RING_PATH}"/>{lines}</g>')
    else:
        body = (f'<defs><clipPath id="co-arc"><polygon points="42.6,50 112.6,-20 130,-20 130,120 112.6,120"/>'
                f'</clipPath>{GAPS}</defs><g mask="url(#co-gaps)">'
                f'<path fill="{hexc(TILE)}" fill-rule="evenodd" d="{RING_PATH}"/>'
                f'<path fill="{hexc(INK)}" fill-rule="evenodd" clip-path="url(#co-arc)" d="{RING_PATH}"/></g>'
                f'<g fill="{hexc(INK)}">{lines}</g>')
    return ('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100" role="img" aria-label="Cold Open">'
            + body + "</svg>\n")


def draw(size):
    """RGBA rows of the icon at `size`, 4x4 supersampled."""
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
    """An .icns of PNG entries, by the type codes macOS reads for each size.

    Only the slots that hold PNG inside an app bundle (release.md §12):
    icp4, icp5 and icp6 are raw-pixel slots there, and a PNG in them drew
    as a garbled purple tile in small views. macOS scales 16 and 32 down
    from ic11 and ic12."""
    kinds = [
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
