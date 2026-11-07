# Typesetter

**Signed-distance-field text for Godot 4.7, with a TextMesh Pro-style API.**

[Splatterface Games](https://www.splatterfacegames.com/) · [Repository](https://github.com/splatterfacegames/godot-typesetter) · [MIT License](LICENSE)

Typesetter is a Godot-native text renderer modelled on TextMesh Pro's feature
set. Its public API is meant to map one-to-one onto TextMesh Pro's under renamed
identifiers (`TextMeshProUGUI` → `TypesetterLabel`, `TextMeshPro` →
`TypesetterText3D`, `TMP_FontAsset` → `TypesetterFontAsset`, members in
snake_case).

**Godot 4.7+ · Rust (gdext) core + GDScript API · MIT licensed**

## Status

Early development; not usable yet. What exists today:

- **Native core** (`native/`, Rust):
  - font loading via skrifa, with glyph metrics that reproduce Unity's to the
    1/64 px;
  - atlas generation in TextMesh Pro's documented render modes (SDF, SDFAA,
    SDF8/16/32, bitmap, hinted variants) plus MSDF/MTSDF via fdsm;
  - MaxRects/Skyline packing and static or dynamic atlases;
  - a rich-text tokenizer;
  - a layout engine.
- **Layout parity:** on the 60 black-box cases in `bench/cases`, glyph
  positions, line breaks, preferred and rendered sizes, overflow and
  auto-size results match Unity TextMesh Pro to within 0.02 units
  (`cargo run --release --example parity` in `native/`).
- **Godot bindings in progress:** `TypesetterFontAsset`, `TypesetterTextCore`
  (layout, textInfo, meshes) and the distance-field shaders compile.

Not built yet: the text nodes (`TypesetterLabel`, `TypesetterText3D`), the input
field, editor tooling, shader calibration against Unity's renders, tests in
Godot, benchmarks, CI and the product site. See [docs/research.md](docs/research.md)
for the survey, architecture and milestone plan.

## Building

```console
python native/build.py      # builds the Rust core into addons/typesetter/bin
```

## Validation

`tools/unity-dump` is a Unity 6000.3 batchmode project that drives TextMesh Pro
through its public API and records what it produces: textInfo, preferred and
rendered values, generated atlases and PNG renders. Its output is committed
under `bench/unity`, so comparisons run without Unity.

```console
python tools/make_cases.py               # regenerate bench/cases
python tools/unity-dump/run.py --render  # regenerate bench/unity (needs Unity)
```

## Inspiration and provenance

Typesetter was inspired by **TextMesh Pro**, but it is a **clean-room
implementation** created by **Claude Opus 5.5 (Anthropic)**. No TextMesh Pro,
com.unity.ugui, com.unity.textcore or UI Toolkit source code was read, and no
Unity assemblies were decompiled. The implementation was written from:

- Unity's public manual and scripting-API documentation;
- published papers and open-source projects that are not Unity's;
- black-box measurements of Unity's output.

No Unity shaders, fonts, sprites or other assets are included.

Typesetter is not affiliated with, endorsed by, or sponsored by Unity
Technologies. TextMesh Pro and Unity are trademarks of their respective owner.
References to TextMesh Pro describe inspiration and API correspondence only.

## License

Typesetter is available under the [MIT License](LICENSE). Third-party software
and fonts included in the repository remain subject to their own licenses:

- [GUT](addons/gut/LICENSE.md) (MIT);
- [Inter](tests/fonts/Inter-OFL.txt) (SIL Open Font License 1.1).
