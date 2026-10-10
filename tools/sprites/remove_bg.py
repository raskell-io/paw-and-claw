#!/usr/bin/env python3
"""Make a sprite's flat background transparent.

Flood-fills from the image border across pixels close to the border color and
clears them. A sprite's dark outline stops the fill, so the character itself is
untouched. Use it when a generator returns an opaque background.

Usage:
  python3 tools/sprites/remove_bg.py IN.png [OUT.png] [--tolerance 24]

Writes OUT.png (defaults to overwriting IN.png). Standard library only.
"""

import argparse
import struct
import zlib
from collections import deque
from pathlib import Path


def read_png(path):
    data = Path(path).read_bytes()
    assert data[:8] == b"\x89PNG\r\n\x1a\n", "not a PNG"
    pos, idat, width, height, color_type = 8, b"", 0, 0, 0
    while pos < len(data):
        length, = struct.unpack(">I", data[pos:pos + 4])
        kind, chunk = data[pos + 4:pos + 8], data[pos + 8:pos + 8 + length]
        if kind == b"IHDR":
            width, height, depth, color_type = struct.unpack(">IIBB", chunk[:10])
            assert depth == 8 and color_type in (2, 6), "only 8-bit RGB/RGBA PNGs are supported"
        elif kind == b"IDAT":
            idat += chunk
        pos += 12 + length
    channels = 4 if color_type == 6 else 3
    raw = zlib.decompress(idat)
    stride = width * channels + 1
    prev = bytearray(width * channels)
    pixels = []
    for y in range(height):
        filter_type = raw[y * stride]
        line = bytearray(raw[y * stride + 1:(y + 1) * stride])
        for i in range(len(line)):
            a = line[i - channels] if i >= channels else 0
            b = prev[i]
            c = prev[i - channels] if i >= channels else 0
            if filter_type == 1:
                line[i] = (line[i] + a) & 255
            elif filter_type == 2:
                line[i] = (line[i] + b) & 255
            elif filter_type == 3:
                line[i] = (line[i] + (a + b) // 2) & 255
            elif filter_type == 4:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                line[i] = (line[i] + (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 255
        prev = line
        for x in range(width):
            px = line[x * channels:(x + 1) * channels]
            pixels.append(tuple(px) + ((255,) if channels == 3 else ()))
    return width, height, pixels


def write_png(path, width, height, pixels):
    raw = b"".join(b"\0" + bytes(c for p in pixels[y * width:(y + 1) * width] for c in p) for y in range(height))

    def chunk(kind, body):
        return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", zlib.crc32(kind + body) & 0xFFFFFFFF)

    Path(path).write_bytes(
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )


def remove_background(width, height, pixels, tolerance):
    corners = [pixels[0], pixels[width - 1], pixels[(height - 1) * width], pixels[-1]]
    background = max(set(corners), key=corners.count)

    def close(p):
        return p[3] > 0 and max(abs(p[i] - background[i]) for i in range(3)) <= tolerance

    out = list(pixels)
    seen = bytearray(width * height)
    queue = deque(i for i in range(width * height)
                  if (i % width in (0, width - 1) or i // width in (0, height - 1)))
    cleared = 0
    while queue:
        i = queue.popleft()
        if seen[i] or not close(pixels[i]):
            continue
        seen[i] = 1
        out[i] = (0, 0, 0, 0)
        cleared += 1
        x, y = i % width, i // width
        for nx, ny in ((x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)):
            if 0 <= nx < width and 0 <= ny < height:
                queue.append(ny * width + nx)
    return out, background, cleared


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("input")
    parser.add_argument("output", nargs="?")
    parser.add_argument("--tolerance", type=int, default=24, help="max per-channel difference from the background color")
    args = parser.parse_args()
    width, height, pixels = read_png(args.input)
    out, background, cleared = remove_background(width, height, pixels, args.tolerance)
    write_png(args.output or args.input, width, height, out)
    print(f"{width}x{height}: cleared {cleared} background pixels (color {background[:3]})")


if __name__ == "__main__":
    main()
