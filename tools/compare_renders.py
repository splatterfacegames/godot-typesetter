#!/usr/bin/env python3
"""Compares Typesetter renders (bench/godot) with TextMesh Pro renders
(bench/unity) pixel by pixel.

    python tools/compare_renders.py [--only a,b] [--diff out_dir] [--json report.json]

Per case it prints the mean absolute error (0-255, RGB), the share of pixels
differing by more than 8 and by more than 32, and PSNR. With --diff it writes
amplified difference images.
"""
import argparse
import json
import math
import os
import struct
import zlib

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def read_png(path):
    data = open(path, "rb").read()
    assert data[:8] == b"\x89PNG\r\n\x1a\n", path
    pos = 8
    idat = b""
    w = h = 0
    ct = bd = 0
    while pos < len(data):
        n = struct.unpack(">I", data[pos:pos + 4])[0]
        t = data[pos + 4:pos + 8]
        body = data[pos + 8:pos + 8 + n]
        if t == b"IHDR":
            w, h, bd, ct = struct.unpack(">IIBB", body[:10])
        elif t == b"IDAT":
            idat += body
        pos += 12 + n
    assert bd == 8, "8-bit PNG expected"
    ch = {6: 4, 2: 3, 0: 1, 4: 2}[ct]
    raw = zlib.decompress(idat)
    stride = w * ch
    out = bytearray(h * stride)
    prev = bytearray(stride)
    p = 0
    for y in range(h):
        f = raw[p]
        p += 1
        line = bytearray(raw[p:p + stride])
        p += stride
        for x in range(stride):
            a = line[x - ch] if x >= ch else 0
            b = prev[x]
            c = prev[x - ch] if x >= ch else 0
            if f == 1:
                line[x] = (line[x] + a) & 255
            elif f == 2:
                line[x] = (line[x] + b) & 255
            elif f == 3:
                line[x] = (line[x] + (a + b) // 2) & 255
            elif f == 4:
                pa, pb, pc = abs(b - c), abs(a - c), abs(a + b - 2 * c)
                pr = a if pa <= pb and pa <= pc else (b if pb <= pc else c)
                line[x] = (line[x] + pr) & 255
        out[y * stride:(y + 1) * stride] = line
        prev = line
    return w, h, ch, out


def write_png(path, w, h, rgb):
    raw = b"".join(b"\x00" + bytes(rgb[y * w * 3:(y + 1) * w * 3]) for y in range(h))
    def chunk(t, b):
        return struct.pack(">I", len(b)) + t + b + struct.pack(">I", zlib.crc32(t + b) & 0xFFFFFFFF)
    open(path, "wb").write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
                           + chunk(b"IDAT", zlib.compress(raw, 6)) + chunk(b"IEND", b""))


def compare(a_path, b_path, diff_path=None, flip_b=False):
    wa, ha, ca, a = read_png(a_path)
    wb, hb, cb, b = read_png(b_path)
    if (wa, ha) != (wb, hb):
        return None
    total = 0
    over8 = over32 = 0
    sq = 0
    diff = bytearray(wa * ha * 3) if diff_path else None
    for y in range(ha):
        yb = (ha - 1 - y) if flip_b else y
        for x in range(wa):
            ia = (y * wa + x) * ca
            ib = (yb * wb + x) * cb
            m = 0
            for k in range(3):
                d = abs(a[ia + k] - b[ib + k])
                total += d
                sq += d * d
                m = max(m, d)
            if m > 8:
                over8 += 1
            if m > 32:
                over32 += 1
            if diff is not None:
                o = (y * wa + x) * 3
                diff[o] = min(255, a[ia] // 3 + (m * 4 if a[ia + 1] > b[ib + 1] else 0))
                diff[o + 1] = min(255, a[ia + 1] // 3)
                diff[o + 2] = min(255, a[ia + 2] // 3 + (m * 4 if a[ia + 1] <= b[ib + 1] else 0))
    n = wa * ha
    mse = sq / (n * 3)
    if diff is not None:
        write_png(diff_path, wa, ha, diff)
    return {"mae": total / (n * 3), "over8": over8 / n, "over32": over32 / n,
            "psnr": 99.0 if mse == 0 else 10 * math.log10(255 * 255 / mse)}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--godot", default=os.path.join(REPO, "bench", "godot"))
    ap.add_argument("--unity", default=os.path.join(REPO, "bench", "unity"))
    ap.add_argument("--only", default="")
    ap.add_argument("--diff", default="")
    ap.add_argument("--json", default="")
    a = ap.parse_args()
    only = set(x for x in a.only.split(",") if x)
    if a.diff:
        os.makedirs(a.diff, exist_ok=True)
    report = {}
    for fn in sorted(os.listdir(a.godot)):
        if not fn.endswith(".png"):
            continue
        name = fn[:-4]
        if only and name not in only:
            continue
        up = os.path.join(a.unity, fn)
        if not os.path.exists(up):
            continue
        r = compare(up, os.path.join(a.godot, fn), os.path.join(a.diff, fn) if a.diff else None)
        if r is None:
            print("%-28s size mismatch" % name)
            continue
        report[name] = r
        print("%-28s mae %6.2f  >8 %6.2f%%  >32 %6.2f%%  psnr %5.1f" % (name, r["mae"], 100 * r["over8"], 100 * r["over32"], r["psnr"]))
    if report:
        m = sum(r["mae"] for r in report.values()) / len(report)
        print("mean mae %.3f over %d cases" % (m, len(report)))
    if a.json:
        json.dump(report, open(a.json, "w"), indent=1)


if __name__ == "__main__":
    main()
