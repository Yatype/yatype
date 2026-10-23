#!/usr/bin/env python3
"""Собирает .ico для Windows из PNG-картинок разных размеров.

    python3 tools/png_to_ico.py out.ico icon16.png icon32.png ... icon256.png

Формат ICO: заголовок, таблица картинок, сами PNG подряд. Windows Vista и
новее понимают PNG внутри ICO.
"""

import struct
import sys


def png_size(data):
    # Ширина и высота — в чанке IHDR, сразу после 8-байтной подписи PNG.
    return struct.unpack(">II", data[16:24])


def main():
    out, pngs = sys.argv[1], [open(p, "rb").read() for p in sys.argv[2:]]
    header = struct.pack("<HHH", 0, 1, len(pngs))
    offset = 6 + 16 * len(pngs)
    table, body = b"", b""
    for data in pngs:
        w, h = png_size(data)
        table += struct.pack("<BBBBHHII", w % 256, h % 256, 0, 0, 1, 32, len(data), offset)
        offset += len(data)
        body += data
    open(out, "wb").write(header + table + body)


if __name__ == "__main__":
    main()
