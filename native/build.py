#!/usr/bin/env python3
"""Build the Typesetter native core (release) and install it into the addon.

    python native/build.py            # host target
    python native/build.py --target x86_64-unknown-linux-gnu

The addon requires this library: atlas generation, rich-text parsing, layout
and mesh building all live in the native core.
"""
import argparse
import platform
import shutil
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
BIN = HERE.parent / "addons" / "typesetter" / "bin"
OUT = {  # rust target triple -> (artifact, installed name)
    "x86_64-pc-windows-msvc": ("typesetter_native.dll", "typesetter_native.windows.x86_64.dll"),
    "x86_64-unknown-linux-gnu": ("libtypesetter_native.so", "libtypesetter_native.linux.x86_64.so"),
    "aarch64-apple-darwin": ("libtypesetter_native.dylib", "libtypesetter_native.macos.arm64.dylib"),
}


def host() -> str:
    s = platform.system()
    if s == "Windows":
        return "x86_64-pc-windows-msvc"
    if s == "Darwin":
        return "aarch64-apple-darwin"
    return "x86_64-unknown-linux-gnu"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--target", default=host())
    a = ap.parse_args()
    if a.target not in OUT:
        print("unsupported target %s" % a.target)
        return 1
    r = subprocess.run(["cargo", "build", "--release", "--target", a.target], cwd=HERE)
    if r.returncode != 0:
        return r.returncode
    art, name = OUT[a.target]
    BIN.mkdir(parents=True, exist_ok=True)
    dst = BIN / name
    try:
        shutil.copy2(HERE / "target" / a.target / "release" / art, dst)
    except PermissionError:
        # The editor holds the library open on Windows: copy beside it and swap on next start.
        shutil.copy2(HERE / "target" / a.target / "release" / art, dst.with_suffix(dst.suffix + ".new"))
        print("library in use; wrote %s.new (close Godot and rename)" % dst)
        return 0
    print("installed %s" % dst)
    return 0


if __name__ == "__main__":
    sys.exit(main())
