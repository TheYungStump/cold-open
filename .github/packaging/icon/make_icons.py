"""Makes the app's icons for every package (docs/specs/release.md §3):

    python3 .github/packaging/icon/make_icons.py [--from FOLDER]

Writes, beside this script:
- png/<size>.png at 16, 24, 32, 48, 64, 128, 256, 512 and 1024 pixels
  (the Linux hicolor icons, and the sources of the two below);
- coldopen.ico (Windows: the .exe and the installer), holding 16 to 256;
- coldopen.icns (macOS), holding 16 to 1024.

Without --from it draws the stand-in: a page on a dark tile with a red bar,
plain on purpose, until the project owner's artwork arrives. With --from,
it takes <size>.png files at those sizes from FOLDER (exported from the
artwork) and only packs them. Pure Python, no image library.
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
LINE = (150, 156, 170)


def rounded(x, y, x0, y0, x1, y1, r):
    """Whether (x, y) is inside the rectangle with corners rounded by r."""
    if not (x0 <= x <= x1 and y0 <= y <= y1):
        return False
    cx = min(max(x, x0 + r), x1 - r)
    cy = min(max(y, y0 + r), y1 - r)
    return (x - cx) ** 2 + (y - cy) ** 2 <= r * r


def colour_at(u, v):
    """The stand-in's colour at (u, v) in 0..1, or None where transparent."""
    if not rounded(u, v, 0.04, 0.04, 0.96, 0.96, 0.2):
        return None
    if rounded(u, v, 0.26, 0.18, 0.74, 0.84, 0.04):
        if 0.18 <= v <= 0.31:
            return BAR
        for top in (0.42, 0.52, 0.62, 0.72):
            right = 0.62 if top in (0.52, 0.72) else 0.66
            if top <= v <= top + 0.035 and 0.33 <= u <= right:
                return LINE
        return PAGE
    return TILE


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
    print("wrote", ", ".join(f"{s}.png" for s in SIZES), "coldopen.ico, coldopen.icns")


if __name__ == "__main__":
    main()
