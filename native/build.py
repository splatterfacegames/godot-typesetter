#!/usr/bin/env python3
"""Build the Typesetter native core (release) and install it into the addon.

    python native/build.py            # host target
    python native/build.py --target x86_64-unknown-linux-gnu
    python native/build.py --web      # needs nightly Rust, emsdk 4.0.11 and GODOT4_BIN

The addon requires this library: atlas generation, rich-text parsing, layout
and mesh building all live in the native core.
"""
import argparse
import os
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
    "wasm32-unknown-emscripten": ("typesetter_native.wasm", "typesetter_native.wasm"),
}
WEB = "wasm32-unknown-emscripten"


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
    ap.add_argument("--web", action="store_true", help="build the single-threaded web library")
    a = ap.parse_args()
    if a.web:
        a.target = WEB
    if a.target not in OUT:
        print("unsupported target %s" % a.target)
        return 1
    if a.target == WEB:
        # Emscripten side module: nightly for -Zbuild-std; gdext reads the API from GODOT4_BIN.
        # Panics abort in a Godot wasm build anyway; with panic=abort nothing needs
        # Wasm exception handling, which Godot's web templates do not enable.
        cmd = ["cargo", "+nightly", "build", "-Zbuild-std=std,panic_abort", "--release", "--target", WEB, "--no-default-features", "--features", "web"]
        env = dict(os.environ, CARGO_PROFILE_RELEASE_PANIC="abort")
    else:
        cmd = ["cargo", "build", "--release", "--target", a.target]
        env = None
    r = subprocess.run(cmd, cwd=HERE, env=env)
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
