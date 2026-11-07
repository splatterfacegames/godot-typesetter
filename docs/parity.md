# Parity with TextMesh Pro

Typesetter is checked against TextMesh Pro's measured output on Unity 6000.3.24f1.
`tools/unity-dump` drives TextMesh Pro through its public API and records
`textInfo`, preferred and rendered values, atlases, materials and PNG renders into
`bench/unity`. The same cases, in `bench/cases`, run through Typesetter. Both sides
use open-licensed fonts (Inter 3.009 and a Noto Sans JP subset).

```console
cargo run --release --example parity                  # in native/: layout core
godot --headless -s addons/gut/gut_cmdln.gd -gexit    # GUT, including the shipped nodes
godot --path . -s res://tests/render/render_cases.gd  # renders; needs a GPU
python tools/compare_renders.py                       # pixels
python tools/compare_atlases.py                       # atlas texels
python tools/unity-dump/run.py --render --only a,b    # new ground truth; needs Unity
```

## Results

Measured on Windows with Godot 4.7.2 and an Intel Xeon W-2135.

| What | Measure | Result |
| --- | --- | --- |
| Layout core | Glyph positions, lines, words, links and sizes within 0.02 units | 198 of 206 cases |
| Shipped nodes | The same cases through `TypesetterLabel` and `TypesetterText3D` | All but the 8 known gaps below |
| Atlases | Mean texel difference, per render mode | 0.05 to 0.90 of 255 |
| Rendered pixels | Mean absolute difference, per case | Median 0.075 of 255, mean 0.27, over 227 cases |
| Shader effects | The same, over the 21 `fx_` cases | Mean 0.40 of 255 |

| Atlas mode | Mean difference (of 255) |
| --- | --- |
| SMOOTH | 0.20 |
| SMOOTH_HINTED | 0.29 |
| RASTER, RASTER_HINTED | 0.49 |
| SDF | 0.33 |
| SDF8 | 0.21 |
| SDF16 | 0.06 |
| SDF32 | 0.05 |
| SDFAA | 0.90 |
| SDFAA_HINTED | 0.86 |

| Shader effect | Mean difference (of 255) |
| --- | --- |
| Plain, face colour, dilate, softness | 0.05 to 0.21 |
| Outline (with width, softness or dilate) | 0.12 to 0.18 |
| Underlay (normal, soft, dilated, inner) | 0.09 to 0.18 |
| Bold | 0.20 |
| Glow | 0.46 |
| Glow, inner | 0.80 |
| Bevel | 1.03 |
| Outline, underlay and glow together | 1.12 |
| Glow with an offset | 2.80 |

## Known gaps

The 8 layout cases are listed in `KNOWN_GAPS` in `native/examples/parity.rs` and
`tests/test_parity.gd`. CI fails on any other divergence.

| Cases | Cause |
| --- | --- |
| `rt_escapes` | Unity falls back to its bundled emoji sprite asset, which isn't available to a clean-room harness |
| `tp_missing_glyph` | Unity falls back to its bundled Liberation Sans, for the same reason |
| `rt_unknown`, `tp_prefctx_2`, `tp_prefctx_8`, `tp_prefctx_9` | After malformed tags, TextMesh Pro's preferred width includes extra bold spacing. Not reproduced. It affects measurement only, not glyph positions |
| `tp_open_bad_values`, `tp_italic_sizes` | Italic shear on superscript text differs by about 1 px at 30 pt |

Render outliers with a known cause:

| Case | Mean difference | Cause |
| --- | --- | --- |
| `tp_linked` | 14.71 | The render harness builds only the primary component; Unity's image includes the linked one |
| `rt_sprite`, `rt_sprite_face` | 5.64, 1.96 | Unity's runtime sprite asset maps every sprite to one glyph, and the harness doesn't reproduce its sheet sampling |
| `rt_escapes`, `tp_missing_glyph` | 3.69, 2.35 | The missing fallback assets above |
| `fx_glow_offset` | 2.80 | The fit below doesn't capture how Unity fills between the face and an offset glow |
| `tmp3d_wrap` | 1.11 | Godot's 3D pipeline blends in linear space, while the harness's Unity project uses gamma |

**Fitted effects.** Glow and bevel are fitted to a few cases, not derived from a
known formula. Expect them to be close at the tested settings and less so at others.
Bevel covers one set of parameters. Local lighting, bump maps and environment maps
compile and run but have no Unity comparison.

**Not compared.** The Mobile and Bitmap shaders have no dedicated shader-feature
cases. Default runtime-created Unity assets use the Mobile shader, and plain text
renders identically under it and the full shader. MSDF and MTSDF have no Unity
equivalent to compare with. The input field is covered by GUT only. The editor tooling
has not been exercised in a live editor session.

`textInfo.spriteCount` differs on purpose. Unity reports 0 even with sprites in the
text, and Typesetter reports the real number.

## Measured behaviour

These rules came from Unity's output, and each is covered by a case.

- **Kerning.** Dynamic assets read GPOS `kern` lookups with no lookup flags, so
  Noto Sans JP kerns and Inter (IgnoreMarks) doesn't. Only the text's primary font
  picks up new pairs.
- **Face metrics.** The cap and mean lines round up. Superscript and subscript come
  from the ascent and descent at size 0.5, not OS/2. Strikethrough sits at 0.4 of the
  mean line. The underline offset is the font's minus half the thickness.
- **Underline and strikethrough.** The offset is the band's top edge, and the band
  shows 0.88 of the thickness.
- **Highlights.** They span the glyph quads, from descender to ascender, at half the
  colour's alpha. `FontStyles.Highlight` alone is white.
- **Bold.** `weight_bold × 0.125` dilates the edge. The glyph's UVs extend by
  `bold_style / 4 × atlas_padding` on both axes.
- **Soft outlines.** Softness acts only on the outer edge.
- **Inner underlay.** It sits behind the face, so an opaque face hides it.
- **Sprites.** With face info, scale is `size / point size`. Without it, the glyph
  fills the font's ascent line. A `<sprite>` tag reports U+E000 + index, and sprites
  aren't word characters. A tag's `color` tints the vertices and leaves
  `characterInfo.color` alone.
- **Hinted modes.** The bitmap is the unhinted one. Only the metrics snap to pixels.
- **Plain SDF and SDF8/16/32.** Plain SDF is a two-sided distance transform of a 1×
  binary mask with dropout control. The k× modes sample the high-resolution field at
  block centres, with the frame anchored at the first lit pixel.

## Performance

`godot --path . -s res://tests/bench/benchmarks.gd` writes `bench/results`.

| Measure | Result |
| --- | --- |
| Atlas, ASCII at 90 pt: SMOOTH, RASTER | 4 ms |
| Atlas: SDF, SDFAA | 7 to 8 ms |
| Atlas: MTSDF | 107 ms |
| Atlas: SDF8, SDF16, SDF32 | 228 ms, 1.1 s, 5.0 s |
| Layout per 1,000 characters | 1.3 to 1.4 ms |
| Layout and mesh per 1,000 characters | 1.9 ms |
| `TypesetterLabel` text change per 1,000 characters | 4.8 ms |
| `TypesetterText3D` text change per 1,000 characters | 5.6 ms |
| Godot `Label` text change per 1,000 characters | 0.55 ms |
| 100 labels changing every frame, Typesetter | 30 ms per frame |
| 100 labels changing every frame, `Label` | 16.3 ms per frame (held at the 60 Hz cap, so a ceiling rather than its true cost) |

A label's text change costs far more than its layout and mesh. The difference is
mostly GDScript syncing settings into the core on each update, which is the first
place to optimise. Text that changes rarely isn't affected. The `Label3D` benchmark
isn't reported because it returned before Godot had done the work.
