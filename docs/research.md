# GPU text rendering for Godot: survey and recommended architecture

Status: research for step 1 (November 2026). Working name of the addon: **Typesetter**
(provisional, see §9). This document surveys what exists and what is state of the art for GPU
text, then recommends an architecture for a Godot 4.7 addon with TextMeshPro's feature set and a
renamed, one-to-one mapping of TextMeshPro's public API.

**Clean-room note.** Everything here comes from published papers, open-source projects that are
not Unity's, Godot's own source (MIT), and Unity's *public* manual and scripting-API pages. No
TextMeshPro, com.unity.ugui, com.unity.textcore or UI Toolkit source was read, and nothing was
decompiled. Where this document describes TextMeshPro behaviour it cites the Unity manual page it
came from, or marks the point as something to measure black-box in Unity.

Contents: §1 single-channel SDF · §2 MSDF/MTSDF · §3 direct Bézier and coverage · §4 Godot's
built-in text · §5 other engines and Rust crates · §6 trade-offs · §7 recommendation ·
§8 validation plan · §9 naming and API mapping · §10 risks and open questions · §11 milestones.

---

## 1. Single-channel signed distance fields (TextMeshPro's approach)

### 1.1 The technique

Green's 2007 Valve paper, *Improved Alpha-Tested Magnification for Vector Textures and Special
Effects*, is the origin of SDF text in games
([PDF](https://steamcdn-a.akamaihd.net/apps/valve/2007/SIGGRAPH2007_AlphaTestedMagnification.pdf)).
It stores the distance to the nearest edge in an 8-bit channel, generated from a high-resolution
binary image (4096² down to as little as 64²). A "spread" maps distance into 0..1. The paper
renders with an alpha test at 0.5 or with `smoothstep` between two thresholds, the width
optionally taken from screen-space derivatives. It also derives the effects TextMeshPro later
productised: outlines (a distance band), glows (the 0..0.5 region) and drop shadows (a second,
offset lookup). The spread caps how far any effect can reach. Its weakness is that bilinear
interpolation of a single distance channel rounds sharp corners. The paper notes that ANDing two
channels keeps corners sharp, which is the seed of MSDF.

### 1.2 Generating the distance field

| Algorithm | Input | Exactness | Cost | Source |
|---|---|---|---|---|
| Brute-force neighbourhood search | binary raster | exact within the search radius | O(n·r²) | Green 2007 |
| Danielsson 4SED/8SED; 8SSEDT (two grids, one per sign) | binary raster | small errors at some configurations | linear | [Danielsson 1980](https://perso.telecom-paristech.fr/bloch/ANIM/Danielsson.pdf), [Leymarie 1992](https://pages.cs.wisc.edu/~dyer/cs766/readings/leymarie-cvgip92.pdf), [Mitton's write-up](http://www.codersnotes.com/notes/signed-distance-fields/) |
| Dead reckoning (Grevera 2004) | binary raster | better than chamfer | linear | [ResearchGate](https://www.researchgate.net/publication/222702633_The_Dead_reckoning_signed_distance_transform) |
| Felzenszwalb & Huttenlocher 2012 | binary raster (or any sampled function) | **exact** EDT of the samples | linear, two separable 1-D passes | [ToC 8:415](https://theoryofcomputing.org/articles/v008a019/), [PDF](https://cs.brown.edu/people/pfelzens/papers/dt-final.pdf); used by [Mapbox TinySDF](https://github.com/mapbox/tiny-sdf) |
| Gustavson & Strand 2011 (anti-aliased EDT, "edtaa3") | anti-aliased coverage raster | sub-pixel edges; about as good as a binary EDT on 16×16 supersampled input | ~10× a binary EDT | [VCL](http://vcl.itn.liu.se/publications/2011/GS11/) |
| Analytic distance to the outline | Bézier contours | **exact** | per texel × nearby curves; needs a spatial index | msdfgen `sdf` mode ([repo](https://github.com/Chlumsky/msdfgen)); FreeType 2.11+ `FT_RENDER_MODE_SDF` ([design doc](https://freetype.org/freetype2/docs/design/design-5.html)); `stbtt_GetGlyphSDF` (wrong where contours overlap: [stb#1524](https://github.com/nothings/stb/issues/1524)) |

There are two families. Raster-then-EDT is simple, robust to overlapping contours (the raster
resolves the winding), and can be made accurate by oversampling. Analytic methods are exact and
need no oversampling, but must handle overlapping contours, which variable fonts and many CJK
fonts contain, by computing the sign from the non-zero winding rule rather than per contour.

**What TextMeshPro documents** (Font Asset Creator manual,
[ugui 2.0](https://docs.unity3d.com/Packages/com.unity.ugui@2.0/manual/TextMeshPro/FontAssetsCreator.html)):
- **Render modes:** `SMOOTH`, `RASTER`, `SMOOTH_HINTED`, `RASTER_HINTED` (bitmap); `SDF` ("slower but more accurate … no oversampling"); `SDFAA` and `SDFAA_HINTED` ("faster, less accurate"); `SDF8`, `SDF16`, `SDF32` ("accurate SDF generation with 8×/16×/32× oversampling", SDF32 for small or complex glyphs).
- **Size and spacing:** Sampling Point Size (or Auto Sizing, the largest size that fits); Padding ("5 is often suitable for a 512×512 texture"); Atlas Resolution.
- **Packing:** Packing Method Optimum or Fast. TextCore's public `GlyphPackingMode` enum ([API](https://docs.unity3d.com/ScriptReference/TextCore.LowLevel.GlyphPackingMode.html)) lists BestShortSideFit, BestLongSideFit, BestAreaFit, BottomLeftRule and ContactPointRule, which are the MaxRects heuristic names from Jylänki's survey.
- **Spread:** the Distance Field shader's debug property "Gradient Scale" is documented as **Padding + 1** ([shader manual](https://docs.unity3d.com/Packages/com.unity.ugui@2.0/manual/TextMeshPro/ShadersDistanceField.html)). So the encoded spread, in atlas texels, is tied to the padding.
- **Atlas population:** Static or Dynamic. Dynamic assets keep the source font and rasterise on demand, and `CreateFontAsset(Font, samplingPointSize, atlasPadding, renderMode, w, h, AtlasPopulationMode = Dynamic, enableMultiAtlasSupport = true)` is public API ([TMP_FontAsset](https://docs.unity3d.com/Packages/com.unity.ugui@2.0/api/TMPro.TMP_FontAsset.html)).

The names suggest the mapping: SDF is an exact distance from the outline; SDFAA is an EDT of an
anti-aliased raster at the sampling size (Gustavson-style); SDF8/16/32 is an EDT of a binary
raster oversampled 8/16/32× and then downsampled. Unity does not publish the algorithms. Our
generator will implement these documented *behaviours* with the published algorithms above, and
§8 compares the resulting atlases with Unity's, measured black-box.

### 1.3 Padding, spread and atlas packing

Conventions differ but encode the same thing:
- **Green:** a spread factor.
- **FreeType:** `spread` in pixels, with 128 on the edge ([properties](http://freetype.org/freetype2/docs/reference/ft2-properties.html)).
- **stb_truetype:** an edge value plus a per-pixel scale.
- **msdf-atlas-gen:** `-pxrange` / `-emrange`, with asymmetric variants ([repo](https://github.com/Chlumsky/msdf-atlas-gen)).
- **TextMeshPro:** padding in atlas pixels, with gradient scale = padding + 1.

Typesetter stores spread explicitly per font asset (`atlas_padding`, and a derived
`gradient_scale`) so shaders and effects are size-independent.

Packing follows Jylänki, *A Thousand Ways to Pack the Bin* (2010,
[repo + paper](https://github.com/juj/RectangleBinPack), public domain code):
- **Offline:** MaxRects with the best-short-side-fit heuristic packs best.
- **Online:** Skyline bottom-left comes close and is faster when glyphs arrive one at a time.

Dynamic atlases that add glyphs at runtime are now standard:
- msdf-atlas-gen's `DynamicAtlas`.
- glyphon: etagere packing with LRU eviction ([repo](https://github.com/grovesNL/glyphon)).
- Dear ImGui 1.92: growing atlas ([release](https://github.com/ocornut/imgui/releases/tag/v1.92.0)).
- Godot: per-size glyph cache textures.

TextMeshPro's documented dynamic mode adds glyphs and, with multi-atlas support, adds further
atlas textures instead of repacking.

---

## 2. MSDF and MTSDF

- **Origin:** Chlumský's MSc thesis, *Shape Decomposition for Multi-channel Distance Fields* (CTU Prague, 2015, [PDF](https://dspace.cvut.cz/bitstream/handle/10467/62770/F8-DP-2015-Chlumsky-Viktor-thesis.pdf)), and msdfgen ([repo](https://github.com/Chlumsky/msdfgen), MIT, v1.13 Nov 2025).
- **How it works:** contour edges are coloured so that every corner is formed by edges in different channels. Each channel stores a distance to its own edge subset, and the shader takes `median(r, g, b)`. Bilinear interpolation then preserves sharp corners at low resolution.
- **Edge colouring:** strategies `simple`, `inktrap` (v1.7) and `distance` (v1.9); `-angle` sets the corner threshold.
- **Error correction:** handles the format's characteristic artefacts, texels where the channels disagree after interpolation.
  - Reworked in v1.6 and v1.9; v1.8 added analytic prediction of bilinear-interpolation artefacts.
  - Current modes: `auto-fast | auto-full | auto-mixed` (default; fixes inversion artefacts and errors that don't touch edges), `distance-*`, `edge-*` and `disabled` ([main.cpp](https://raw.githubusercontent.com/Chlumsky/msdfgen/master/main.cpp)).
- **Overlapping contours:** since v1.8 msdfgen can remove self-intersections and overlaps with Skia.
- **MTSDF (v1.7):** MSDF in RGB plus a *true* single-channel SDF in alpha. RGB gives sharp edges; alpha gives correct rounded distances far from the glyph, which is what large outlines, soft underlays and glows need.
- **Practitioners:** Red Blob Games' experiments came down to "halos will always use sdf and outlines always use msdf"; outline quality falls as the range grows, and colours bleed at interior corners where the MSDF and SDF channels disagree ([parameters](https://www.redblobgames.com/x/2437-msdfgen-parameters/), [effects](https://www.redblobgames.com/x/2404-distance-field-effects/)).
- **Costs:**
  - 3× (MSDF) or 4× (MTSDF) the texture memory of single-channel SDF (by format; no benchmarks found).
  - Edge colouring and error correction make generation slower.
  - The shader adds 3 more channels and a median.
  - No hinting, and small sizes look softer: Godot's own docs say MSDF text is less clear at small sizes ([fonts](https://docs.godotengine.org/en/stable/tutorials/ui/gui_using_fonts.html)).
  - Lengyel calls MSDF artefacts "a new class of difficult-to-avoid artifacts for complex glyphs" ([JCGT 2017, p. 32](https://jcgt.org/published/0006/02/02/paper.pdf)).
- **Pure-Rust port:** `fdsm` (0.8.0, MIT, [crates.io](https://crates.io/crates/fdsm)) implements MSDF/MTSDF, with glyph import adapters for ttf-parser and skrifa.

---

## 3. Direct Bézier rendering and coverage atlases

### 3.1 Rendering from the outline on the GPU

- **Loop & Blinn 2005:** curves are triangulated, and a per-pixel implicit test decides inside or outside ([MSR PDF](https://www.microsoft.com/en-us/research/wp-content/uploads/2005/01/p1000-loop.pdf), [GPU Gems 3 ch. 25](https://developer.nvidia.com/gpugems/gpugems3/part-iv-image-effects/chapter-25-rendering-vector-art-gpu)). The Microsoft patent US7564459B2 shows "Expired – Lifetime" on Google Patents. That is Google's assumption, not a legal finding.
- **Slug:** Lengyel, *GPU-Centered Font Rendering Directly from Glyph Outlines*, JCGT 6(2) 2017 ([paper](https://jcgt.org/published/0006/02/02/)).
  - **How it works:** one quad per glyph; the pixel shader computes winding numbers by casting rays against the glyph's quadratic curves. It is robust (no sparkles or streaks) and pixel-exact under projective transforms.
  - **Since 2017:** per the 2026 "A Decade of Slug" post, band splitting and supersampling were removed and "dynamic dilation" added ([blog](https://terathon.com/blog/decade-slug.html)).
  - **Patent:** US 10,373,352 B1 ([Google Patents](https://patents.google.com/patent/US10373352B1/en)). On **17 March 2026** Lengyel wrote that he was "permanently and irrevocably dedicating the Slug patent to the public domain" by filing USPTO form SB/43 to disclaim its remaining term (same blog; [Hackaday coverage](https://hackaday.com/2026/03/20/slug-algorithm-for-on-gpu-rendering-of-fonts-with-bezier-curves-now-in-public-domain/)).
  - **Reference code:** HLSL shaders under MIT/Apache-2.0 at [EricLengyel/Slug](https://github.com/EricLengyel/Slug).
  - **Caveat:** Google Patents still showed the patent as "Active" with no 2026 legal event when checked. The disclaimer's recording at the USPTO is **unverified**, and the commercial Slug Library is unaffected.
- **Vector textures:** Dobbie 2016, per-cell curve lists ([post](https://wdobbie.com/post/gpu-text-rendering-with-vector-textures/)).
- **Stencil winding plus anti-aliasing pass:** Wallace ([post](https://medium.com/@evanwallace/easy-scalable-text-rendering-on-the-gpu-c3f4d782c5ac)).
- **Pathfinder 3:** exact trapezoid coverage, tiled ([repo](https://github.com/servo/pathfinder)). Effectively unmaintained.
- **Vello:** the compute renderer is now "experimental", and work has moved to the sparse-strips renderers; glyphs are vector paths with caching ([repo](https://github.com/linebender/vello)).
- **HarfBuzz hb-gpu** (HarfBuzz 14.x, 2026; [manual](https://harfbuzz.github.io/harfbuzz-hb-gpu.html)): a CPU encoder packs each glyph into a blob, and a fragment shader (GLSL/WGSL/MSL/HLSL) rasterises it with no atlas. "Draw" uses the Slug algorithm and "Paint" does COLRv0/v1. GLyphy now declares itself deprecated in favour of hb-gpu ([repo](https://github.com/behdad/glyphy)). Release dates are only partly verified.
- **Windfoil** (DesLauriers, arXiv Oct 2026, [abs](https://arxiv.org/abs/2610.02468)): a closed-form box-filtered winding number. Preprint, not peer-reviewed.
- **Minification:** Ellis, Hunt & Hart (HPG 2019, [project](https://graphics.cs.illinois.edu/aa-text/)) integrate coverage over the pixel footprint analytically, to keep minified 3D/VR text legible. Any field sampled bilinearly under-samples at extreme minification.

**What direct-Bézier methods give and lack.** Exact edges at every scale and under perspective,
and no atlas memory per size. But they produce *coverage*, not *distance*. TextMeshPro's effect
set (dilate, softness, outlines of arbitrary width, soft and inner underlays, glow, bevel) is
defined on a distance field, so effects would need a per-pixel closest-point solve against the
curves, extra passes, or geometry. They are also the most expensive per pixel.

### 3.2 Coverage atlases (rasterise each glyph at each size)

FreeType or stb_truetype rasterise each glyph at each size into an alpha atlas. glyphon, Dear
ImGui, Bevy and Godot's default path all work this way. This gives the best small-size quality:
hinting, exact coverage and optional LCD subpixel rendering. It is the right choice for
pixel-snapped UI at fixed sizes, but blurs when magnified and needs mipmaps when minified in 3D
(Godot's docs warn that minified 3D text "can look grainy" without mipmaps). TextMeshPro keeps this as the
`SMOOTH`/`RASTER` render modes with a Bitmap shader.

---

## 4. Godot's built-in text (verified against the `4.7-stable` source)

Godot 4.7 was released 18 June 2026; latest stable is 4.7.2 (18 Aug 2026), and 4.8 is in beta
([blog](https://godotengine.org/blog/release/)). This machine has `Godot_v4.7.1-stable_win64.exe`
in `B:/dev/games/godot-builds/`. Source links below use
`G = https://github.com/godotengine/godot/blob/4.7-stable`.

### 4.1 TextServer: shaping and bidi to reuse

TextServerAdvanced bundles HarfBuzz 14.2, ICU 78.3, FreeType, Graphite and msdfgen
([#118785](https://github.com/godotengine/godot/pull/118785)).

What it exposes to GDScript and GDExtension ([TextServer docs](https://docs.godotengine.org/en/stable/classes/class_textserver.html)):
- **Shaping:** `create_shaped_text`, `shaped_text_add_string(text, fonts[], size, features, language, meta)` and `shaped_text_add_object` for inline objects.
- **Bidi:** `shaped_text_set_bidi_override`.
- **Breaks and justification:** line, word and character break queries; `shaped_text_fit_to_width` (justification, including kashida); `shaped_text_overrun_trim_to_width` (ellipsis).
- **Carets and hit testing:** carets, selection, hit testing and grapheme bounds.
- **Font data:** fallback lists, OpenType feature and variation queries.

**The glyph array.** `shaped_text_get_glyphs` returns dictionaries in visual order with `start`,
`end`, `count`, `repeat`, `flags`, `offset`, `advance`, `font_rid`, `font_size`, `index` and
`span_index` (new in 4.7, [#114488](https://github.com/godotengine/godot/pull/114488)). There is no
`cluster` key; `start`/`end` are source-string indices. A GDExtension caller receives a
`TypedArray<Dictionary>`, one allocation per glyph, so shaped runs must be cached and never
re-shaped per frame.

**Glyph outlines.** `font_get_glyph_contours(font, size, index)` returns FreeType outline points
(x, y, tag: on-curve, conic or cubic off-curve) and contour end indices.

Pitfalls (from the code, [text_server_adv.cpp ~L3870](https://github.com/godotengine/godot/blob/4.7-stable/modules/text_server_adv/text_server_adv.cpp#L3870)):
- It loads glyphs without `FT_LOAD_NO_HINTING`, so outlines are hinted at the integer pixel size requested.
- Y is negated.
- The size is clamped to 2048.
- Units-per-em is not exposed.

Usable at a large size, but parsing the font file directly is cleaner (§5).

**Rounding: the cause of godot-uxml's measured gap.** In `_shape_run`, glyph x-offsets and advances
keep HarfBuzz's 1/64-px precision only if
`scale != 1 || subpixel_positioning ∈ {ONE_HALF, ONE_QUARTER} || (AUTO && size <= 20)`.
Otherwise they are `Math::round`ed, with remainders redistributed
([L7199, L7326–7353](https://github.com/godotengine/godot/blob/4.7-stable/modules/text_server_adv/text_server_adv.cpp#L7326-L7353)).
Also:
- y-offsets are always rounded.
- `shaped_text_get_size` applies `ceil()`.
- HarfBuzz shapes **unhinted** (`hb_ft_font_create` defaults to `FT_LOAD_NO_HINTING`), but the HarfBuzz font is created per integer ppem.

The fix is to set `font_set_subpixel_positioning(ONE_QUARTER)` and shape at one reference size,
then scale positions ourselves. MSDF FontFiles already shape once at `msdf_size` and scale, so
their positions are unrounded and size-independent.

### 4.2 Godot's MSDF fonts

`FontFile.multichannel_signed_distance_field`, `msdf_pixel_range` (default 16) and `msdf_size`
(default 48) make TextServer generate a dynamic **MTSDF** atlas with the bundled msdfgen 1.13,
using simple edge colouring ([L970](https://github.com/godotengine/godot/blob/4.7-stable/modules/text_server_adv/text_server_adv.cpp#L970)).

**In 2D:** the MSDF resolve is a specialization constant in `canvas.glsl` that applies **only to
rect commands** (`canvas_item_add_msdf_texture_rect_region`) and runs before user fragment code
([canvas.glsl L608](https://github.com/godotengine/godot/blob/4.7-stable/servers/rendering/renderer_rd/shaders/canvas.glsl#L608)).
- Fill is `median(rgb)` with `fwidth` anti-aliasing. Since 4.5 RGB is used for the fill, which fixed corner rounding ([#109437](https://github.com/godotengine/godot/pull/109437)).
- The outline uses `min(median, a)`, clamped to `px_range/2 − 1`.
- Meshes and triangle arrays get no MSDF treatment.

**In 3D:** Label3D uses a StandardMaterial3D MSDF variant. It is monochrome, draws outline and fill
as separate quads, and a material override discards the MSDF logic
([label_3d.cpp](https://github.com/godotengine/godot/blob/4.7-stable/scene/3d/label_3d.cpp#L364)).

**What's missing:** none of Label, RichTextLabel or Label3D has underlay, glow, bevel, gradients,
face textures or per-glyph vertex access. LabelSettings gained stacked outlines and shadows in
4.5 ([#104731](https://github.com/godotengine/godot/pull/104731)).

**Known issues:**
- Overlapping contours ([#52247](https://github.com/godotengine/godot/issues/52247)).
- Artefacts ([#107137](https://github.com/godotengine/godot/issues/107137)).
- Holes ([#81974](https://github.com/godotengine/godot/issues/81974)).

**godot-uxml's measurement** (`B:/dev/games/godot-uxml/docs/parity.md`, "Pixels"): Godot MSDF text
scored IoU **0.689** against Unity's TextCore text, versus **0.884** for Godot's rasteriser.
- **Rasteriser:** Inter 3.009, grayscale, light hinting, subpixel positioning.
- **Ink:** godot-uxml draws 1.021× Unity's ink.
- **Remaining gaps:** whole-pixel advances (7 of 375 measures are 1 px narrow), whole-pixel line pitch, and half-pixel flooring.
- **Its conclusion:** closing the gap needs Unity's SDF rendering plus fractional advances and line pitch.

The low MSDF score comes from Godot's MSDF parameters (pixel range 16, size 48, no matching of
Unity's sampling size, padding or edge function), not from distance fields as such.

### 4.3 RenderingServer paths for glyph meshes

- **2D:** `canvas_item_add_triangle_array` has no custom attributes. `canvas_item_add_mesh` with a 2D ArrayMesh carries **CUSTOM0 and CUSTOM1** (vec4 each) into CanvasItem shaders ([#86564](https://github.com/godotengine/godot/pull/86564), 4.3). That plus UV, COLOR and VERTEX is enough for TextMeshPro-style per-vertex data.
- **Instance uniforms:** `canvas_item_set_instance_shader_parameter` (4.4) allows up to 16 scalar or vector instance uniforms.
- **Derivatives:** `fwidth`, `dFdx` and `dFdy` work in fragment shaders on every renderer; the coarse and fine variants are not on Compatibility.
- **3D:** ArrayMesh `ARRAY_CUSTOM0..3` (formats up to RGBA float), partial updates with `mesh_surface_update_vertex_region` and `…_attribute_region`, and `GeometryInstance3D` instance uniforms.

### 4.4 Existing Godot addons

None is a runtime SDF text engine with TextMeshPro-style materials:
- **[godot-font-baker](https://github.com/shiena/godot-font-baker):** bakes Godot's MSDF cache.
- **[msdf-atlas-studio](https://github.com/sachinthankachan/msdf-atlas-studio):** offline msdf-atlas-gen GUI.
- **[rich-text-3d-godot](https://github.com/mszylkowski/rich-text-3d-godot):** RichTextLabel rendered in a SubViewport.
- **RichTextEffect animators:** [GodotAnimatedRichText](https://github.com/tioulon/GodotAnimatedRichText), [godot-text_effects](https://github.com/teebarjunk/godot-text_effects), the commercial Febucci Text Animator. All 2D and built-in rendering.
- **Shader tricks on Label:** fragile, because Label UVs are per-glyph ([#23210](https://github.com/godotengine/godot/issues/23210)).

The gap is real.

---

## 5. Other engines and the Rust ecosystem

- **Unreal Engine 5.5+:** runtime SDF/MSDF Slate text, enabled by "Enable Distance Field Font Rasterization" plus a per-Font-Face Distance Field Mode, with quality set by `UI.SlateSDFText.ResolutionLevel` ([Epic docs](https://dev.epicgames.com/documentation/unreal-engine/using-signed-distance-field-text-rendering-in-unreal-engine)). The spread is not configurable ([forum](https://forums.unrealengine.com/t/5-5-4-is-there-a-way-to-dilate-slate-ui-sdf-font-distance-field/2533798)). Older offline "Distance Field Alpha" fonts serve TextRender actors, and Text 3D is extruded geometry ([docs](https://dev.epicgames.com/documentation/unreal-engine/3d-text-actor-in-unreal-engine)). In short, a large engine adopted runtime (M)SDF for UI text in 2024–25.
- **Bevy:** bevy_text 0.20 moved from cosmic-text to **parley + swash** ([#22879](https://github.com/bevyengine/bevy/pull/22879)). It still uses per-size bitmap atlases; SDF exists only in forks.
- **glyphon:** cosmic-text plus an etagere bitmap atlas for wgpu. Coverage, no SDF.
- **Rust crates for a gdext core** (crates.io, Nov 2026):

| Crate | Version | Role |
|---|---|---|
| [skrifa](https://crates.io/crates/skrifa) / read-fonts | 0.49 / 0.46 | Unhinted outlines in font units, metrics, variable-font locations, raw GPOS/GSUB/kern tables. Google Fonts' fontations; actively maintained |
| [fdsm](https://crates.io/crates/fdsm) | 0.8.0 | Pure-Rust msdfgen port: SDF, PSDF, MSDF, MTSDF; error correction |
| [harfrust](https://crates.io/crates/harfrust) | 0.15 | Pure-Rust HarfBuzz port (rustybuzz's successor) for a native shaping path if ever needed |
| [unicode-bidi](https://crates.io/crates/unicode-bidi), [unicode-linebreak](https://crates.io/crates/unicode-linebreak) | | UAX #9 and #14, if layout ever moves fully native |
| [godot](https://crates.io/crates/godot) (gdext) | 0.5.5, `api-4-7` | What godot-uxml uses |

godot-uxml's native core is the model to follow (`B:/dev/games/godot-uxml/native`):
- **Crate:** cdylib, `godot = { version = "0.5.5", features = ["api-4-7"] }`, release profile with thin LTO and `codegen-units = 1`.
- **Build:** `python native/build.py` runs `cargo build --release` and copies the library to `addons/<addon>/bin/<name>.<platform>.<arch>.<ext>`.
- **`.gdextension`:** `compatibility_minimum = 4.7`, and debug and release both point at the optimised library.

---

## 6. Trade-offs

Qualitative synthesis of §§1–5, not measurements.

| | SDF (1-ch) | MSDF | MTSDF | Direct Bézier (Slug, hb-gpu) | Coverage atlas |
|---|---|---|---|---|---|
| Small sizes | fair (no hinting, soft) | fair | fair | good | **best** (hinting) |
| Large sizes | good, rounded corners | excellent | excellent | exact | poor |
| Corners | rounded | sharp | sharp | exact | exact at baked size |
| Outline, underlay, glow, bevel | **excellent**, cheap (within spread) | outlines good, halos poor | **excellent** (RGB edge + A halo) | fill only; effects need more work | needs extra passes |
| 3D / perspective | good with derivative anti-aliasing | good | good | **exact** | poor magnified, needs mips |
| Memory | 1 B/texel | 3 B/texel | 4 B/texel | curve data, no atlas | per-size atlases |
| Generation | cheap–moderate | moderate–high | moderate–high | cheap | cheap, per size |
| Shader cost | **lowest** | low | low–moderate | **highest** | lowest |
| Matches TMP/TextCore pixels | **yes** (same technique) | no | no (RGB edge differs) | no | no (IoU 0.884 today) |
| IP risk | low | low (MIT) | low | low since Mar 2026 (dedication not yet visible on Google Patents) | low |

**Conclusion.** The evidence supports your expectation: distance fields come out on top, and the
reason is the feature set, not only quality. Every TextMeshPro shader feature (dilate, softness,
outline width, underlay offset, dilate and softness, inner underlay, glow, bevel normals from the
distance gradient) is a function of distance. A distance-field atlas makes all of them one or two
texture taps. Direct-Bézier rendering is now patent-clear (subject to the caveat above) and is
the best *fill* renderer for huge or perspective text. But it produces coverage, so the
TextMeshPro effect set would need a closest-point solve per pixel. It also costs the most per
pixel, and it cannot reproduce Unity's SDF pixels, which godot-uxml's parity goal needs.

Between SDF and MTSDF the answer is "both, per font asset":
- **Single-channel SDF must be the default.** It is what TextMeshPro and TextCore render, so it is the only way to match Unity's pixels (godot-uxml's 0.884 → target ≥ 0.95 IoU), and its memory and shader cost suit mobile.
- **MTSDF is the quality option.** It keeps corners sharp at low sampling sizes, and its alpha channel stays a true SDF for halos. One shader handles both: it reads the face edge from `median(rgb)` when the atlas is MTSDF and from `r` when it is SDF, and always takes effects from the true-distance channel.

---

## 7. Recommended architecture

```
                         ┌──────────── GDScript (addons/typesetter) ────────────┐
  TMP-mapped API         │ TypesetterText (base), TypesetterText3D (Node3D),    │
                         │ TypesetterLabel (Control), TypesetterInputField,     │
                         │ TypesetterFontAsset / SpriteAsset / StyleSheet /     │
                         │ ColorGradient (Resources), TextInfo structs, editor  │
                         └───────────────┬───────────────────────────┬──────────┘
                                         │                           │
                     ┌───────────────────▼────────┐     ┌────────────▼───────────┐
  Rust gdext core    │ typesetter_native          │     │ Godot TextServer       │
  (native/)          │  • font parse (skrifa)     │     │  (HarfBuzz/ICU)        │
                     │  • SDF/MTSDF/bitmap gen    │◄───►│  • shaping per run     │
                     │    (analytic, EDT, fdsm)   │     │  • bidi, scripts,      │
                     │  • atlas packing (MaxRects │     │    grapheme/caret data │
                     │    + skyline, dynamic)     │     └────────────────────────┘
                     │  • rich-text parser        │
                     │  • layout engine (TMP      │     ┌────────────────────────┐
                     │    semantics), textInfo    │────►│ RenderingServer        │
                     │  • mesh builder (vertex    │     │  2D: canvas_item_add_  │
                     │    + CUSTOM0/1 streams)    │     │      mesh + material   │
                     └────────────────────────────┘     │  3D: ArrayMesh CUSTOM* │
                                                        └────────────────────────┘
  Shaders (addons/typesetter/shaders): distance_field{,_overlay,_mobile} and bitmap,
  each in canvas_item and spatial flavours generated from shared .gdshaderinc.
```

### 7.1 Font assets and generation (Rust)

**Font loading.** Read the font file with **skrifa**: unhinted outlines in font units, units-per-em,
ascender, descender, line gap, cap and x height, underline and strikeout metrics, cmap, and
variable-font instances. This avoids `font_get_glyph_contours`' hinting, integer sizes and 2048
clamp. The same bytes back a Godot `FontFile` for TextServer shaping, so both sides see one font.

**Render modes**, with the TextMeshPro names kept:
- `SDF`: exact analytic distance to quadratic and cubic contours. Uses a per-glyph segment grid for speed and computes the sign by non-zero winding, so overlapping contours are correct.
- `SDFAA` / `SDFAA_HINTED`: Gustavson–Strand anti-aliased EDT on a coverage raster at the sampling size.
- `SDF8/16/32`: Felzenszwalb–Huttenlocher exact EDT on an 8/16/32× oversampled binary raster, downsampled.
- Bitmap: `SMOOTH`, `RASTER` and their `_HINTED` variants (Godot's FreeType rasteriser through TextServer gives the hinted forms).
- Additions: `MSDF` and `MTSDF` via **fdsm** (auto-mixed error correction, distance-based edge colouring).

**Atlas encoding.** The spread follows TextMeshPro's documented convention: padding p in atlas
pixels, `gradient_scale = p + 1`, encoded so 0.5 is the edge. The exact encoding is calibrated
against Unity's atlases (§8). Glyph rects are padded by p.

**Packing:**
- MaxRects with all five TextCore heuristics (Optimum mode tries sampling sizes by binary search, as Auto Sizing does).
- Skyline for dynamic adds.
- Multi-atlas: a new texture when one fills, matching `isMultiAtlasTexturesEnabled`.

**Dynamic atlases.** Glyphs are added on demand from the text components (`try_add_characters`,
`has_character(…, try_add_character)`). Updates are batched into one `Image` region upload per
frame, and generation runs on a worker thread pool (rayon) with results applied on the main
thread.

**The resource.** `TypesetterFontAsset` stores `face_info`, `glyph_table`, `character_table`,
`font_feature_table` (kerning and pair adjustments extracted from GPOS and `kern`; ligature,
mark-to-base and mark-to-mark records), `atlas_textures`, `creation_settings`,
`atlas_population_mode`, `fallback_font_asset_table` and per-weight `font_weight_table`. Static
assets need no source font at runtime, and the resource saves as `.tres` / `.res`.

### 7.2 Shaping: reuse TextServer where it is sound

Shaping goes through Godot's TextServer:
- **Settings:** subpixel positioning forced to `ONE_QUARTER`, shaping at the asset's sampling point size (or a fixed reference size), positions scaled in float.
- **Glyph runs:** shaped per style run and cached by (text, font, size class, features, direction).
- **What it gives us:** proper bidi (ICU), complex scripts (Arabic, Indic, Thai), font fallback, ligatures and marks, consistent with Godot's own UI.
- **TextMeshPro's per-character model:** the `fontFeatures` list (`kern`, `liga`, `mark`, `mkmk`) maps onto HarfBuzz feature tags. "Simple" Latin layout and Unity's output can therefore be matched by turning features off, and §8 checks which defaults match Unity.

**Fallback if TextServer is too slow.** The `Array<Dictionary>` per glyph is the cost risk. M2
benchmarks it, and if needed the core gains a native harfrust plus unicode-bidi path behind the
same internal trait. HarfBuzz either way, and no shaping is reinvented.

### 7.3 Layout (Rust, TextMeshPro semantics)

Line breaking, wrapping and overflow are *ours*, not TextServer's, because TextMeshPro's rules
differ from ICU's and interact with rich-text state. The rules to implement:
- **Line breaking:** break at spaces and after hyphens; leading and following character tables for CJK.
- **Rich-text interactions:** `<nobr>`, `<pos>`, `<indent>`, `<line-indent>` and `<margin>` affect breaks.
- **Auto-size:** iterates font size between min and max with a character-width adjustment.
- **Overflow modes:** Overflow, Ellipsis, Masking, Truncate, ScrollRect, Page and Linked.

The exact numbers (where line height comes from, how `lineSpacing` units scale, ellipsis placement,
auto-size step) are behaviours to measure black-box in Unity, not to guess (§8).

The output is TextMeshPro's `textInfo` shape: `character_info`, `word_info`, `line_info`,
`page_info`, `link_info` and `mesh_info` (vertices, uvs, uvs2, colors32 per atlas/material). Layout is
float throughout, with no rounding unless the caller asks.

### 7.4 Rendering

- **Per atlas and material:** one mesh per (atlas texture, material) pair, matching TextMeshPro sub-meshes.
- **2D:** a 2D ArrayMesh drawn with `canvas_item_add_mesh` on the node's own canvas item, with the material set via `canvas_item_set_material`. CUSTOM0 carries face-texture UVs and the glyph scale; CUSTOM1 carries the bold flag, underlay and the atlas-page index.
- **3D:** an ArrayMesh with CUSTOM0–3 in a `MeshInstance3D`-like child or through `RenderingServer` instances.
- **Per-character animation:** `update_vertex_data()` pushes edited `mesh_info` back with `mesh_surface_update_vertex_region` / `…_attribute_region`, without re-layout.
- **Anti-aliasing:** derived in the fragment shader from screen-space derivatives of the atlas UV (`fwidth` scaled by `gradient_scale`). Exact for any transform, including perspective, rotation and non-uniform scale, in 2D and 3D. A `perspective_filter` and `scale_ratio` are kept for TextMeshPro parameter compatibility.
- **Shader features** (property names follow the Distance Field manual page):
  - Face: colour, texture with tiling, offset and speed, softness, dilate.
  - Outline: colour, texture, thickness.
  - Underlay: Normal and Inner, with colour, offset, dilate and softness.
  - Bevel: outer and inner, with amount, offset, width, roundness and clamp.
  - Local lighting: light angle, specular colour and power, reflectivity, diffuse and ambient shadow.
  - Bump map (face and outline) and environment cubemap.
  - Glow: colour, offset, inner, outer and power.
  - Masking: off, hard and soft clip rect, with softness.
- **Shader variants:** keyword variants are compiled as separate `.gdshader` files from shared includes (Godot has no runtime keyword system); the Mobile and Bitmap variants are written separately.
- **Material presets:** `ShaderMaterial` resources referencing the font asset's atlas; `<material="…">` selects one.

### 7.5 API layer (GDScript)

Nodes and resources are GDScript classes, as in cameraman, with editor integration, inspectors and
docs comments. They call into the native core. The native core is required: there is no GDScript
fallback for generation or layout. That is unlike godot-uxml, where the GDScript path is the
reference. One implementation means one set of behaviours to validate against Unity. The cost is
that platforms without a built library cannot run the addon (Windows, Linux and macOS first; Web
when gdext's wasm support allows).

### 7.6 Downstream API for godot-uxml

godot-uxml's style applier and layout need text as a service rather than a node.
`TypesetterTextLayout` (RefCounted) provides it:

```gdscript
var layout := TypesetterTextLayout.new()
layout.set_style(style)          # TypesetterTextStyle: font asset, size, colour, font style flags,
                                 # character/word/line/paragraph spacing, alignment, wrapping,
                                 # overflow, outline width/colour, underlay (text-shadow), rich text
layout.set_text("Hello <b>world</b>")
var size: Vector2 = layout.measure(Vector2(max_w, INF))  # fractional; never rounded
layout.layout(Rect2(0, 0, w, h))                         # textInfo available afterwards
layout.draw(canvas_item_rid, Transform2D(), modulate)    # emits meshes into any CanvasItem
var info: TypesetterTextInfo = layout.text_info
```

This maps the USS text properties `-unity-font-definition`, `font-size`, `color`,
`-unity-font-style`, `letter-spacing`, `word-spacing`, `-unity-paragraph-spacing`,
`-unity-text-align`, `white-space`, `text-overflow`, `-unity-text-outline-*`, `text-shadow` and
`-unity-text-generator` onto one style object. godot-uxml calls `measure` from Yoga-style
measure callbacks and `draw` from its element's `_draw`. The nodes use the same object
internally, so the API is not a second implementation. godot-uxml itself is not modified in this
thread.

---

## 8. Validation plan (black-box Unity ground truth)

**The dump project.** `tools/unity-dump/` is a Unity 6000.3.24f1 batchmode project modelled on
godot-uxml's (`run.py` stages cases, Unity runs `-executeMethod TypesetterDump.Run`):
- It depends on `com.unity.ugui` (which contains TextMeshPro in Unity 6) as a black box.
- TMP Essential Resources are imported *into the dump project only*, are gitignored, and are never opened or copied.
- Fonts are open-licensed on both sides: Inter 3.009 (OFL; metrically Unity's NotInter), Noto Sans plus Noto Sans Arabic and CJK subsets for fallback and bidi, and Liberation Serif.

For each case (a JSON file: text, font, size, rect, wrapping, overflow, alignment, spacing,
material settings, camera) it writes:
1. **textInfo JSON:** per character, the character, index, origin, xAdvance, ascender, descender, baseLine, bottom-left and top-right, scale and line number. Per line: first and last character, ascender, descender, lineHeight, baseline and width. Per word, link and page. Also `preferredWidth/Height`, `GetPreferredValues(w,h)`, `GetRenderedValues()`, `isTextOverflowing`, `firstOverflowCharacterIndex`.
2. **The generated atlas** for each render mode and padding, created through the public `TMP_FontAsset.CreateFontAsset(...)`. We compare atlas pixels with our generator's (black-box output, allowed), which calibrates the spread encoding and checks each mode.
3. **PNG renders** through a camera into a RenderTexture: face only, then each shader feature in isolation (outline, underlay normal and inner, glow, bevel), 2D overlay and 3D perspective.

`tools/compare.py` (and GUT tests that load the committed JSON) check:
- Per-glyph positions within 0.02 px.
- Line breaks identical.
- Preferred and rendered values within 0.05 px.
- Overflow flags and page splits identical.
- Pixels: IoU ≥ 0.97 face mask and mean absolute error per feature, with a stated tolerance per feature.

Results are committed under `bench/unity/` so CI runs without Unity.

**Tests and benchmarks:**
- **GUT, headless:** unit tests for generator modes (distance accuracy against analytic shapes), packing, rich-text parsing (every tag), layout rules, textInfo and per-character mesh edits; integration tests for the nodes.
- **Benchmarks** (`tests/bench/`):
  - Atlas generation time per mode (ASCII and Latin-1 at 90 pt, 1024², padding 9).
  - Layout time per 1k glyphs (plain, rich text, wrapped).
  - Per-frame cost for N static and N animated text objects.
  - The same text in `Label`, `RichTextLabel` and `Label3D` for comparison.
- **CI:** GitHub Actions on Linux with Godot 4.7.2. It builds the Rust core, imports, runs GUT and gdlint, and runs a short benchmark smoke test.

---

## 9. Names and API mapping

**Name candidates** (cameraman style: a person who does the job):
1. **Typesetter** (recommended). The trade closest to what the addon does. Classes: `TypesetterText`, `TypesetterText3D`, `TypesetterLabel`, `TypesetterFontAsset`, `TypesetterInputField`.
2. **Signwriter.** Painted lettering on surfaces, good for world-space text. Classes: `SignwriterText3D`, `SignwriterLabel`, …
3. **Letterer.** Comic-book lettering (balloons, effects, styles). Classes: `LettererText3D`, `LettererLabel`, …

A GitHub search found no existing Godot repos under these names.

**Mapping rule** (the "renamed API"):

| TextMeshPro | Typesetter |
|---|---|
| `TMP_Text` (abstract base) | `TypesetterText` (shared code; GDScript has no multiple inheritance, so the nodes compose a shared `TypesetterTextCore` and forward) |
| `TextMeshPro` (3D) | `TypesetterText3D` (`Node3D` / `GeometryInstance3D`) |
| `TextMeshProUGUI` (UI) | `TypesetterLabel` (`Control`) |
| `TMP_InputField` | `TypesetterInputField` (`Control`) |
| `TMP_FontAsset`, `TMP_SpriteAsset`, `TMP_StyleSheet`, `TMP_ColorGradient`, `TMP_Settings` | `TypesetterFontAsset`, `TypesetterSpriteAsset`, `TypesetterStyleSheet`, `TypesetterColorGradient`, `TypesetterSettings` |
| `TMP_TextInfo`, `TMP_CharacterInfo`, `TMP_LineInfo`, … | `TypesetterTextInfo`, `TypesetterCharacterInfo`, `TypesetterLineInfo`, … |
| enums `TextAlignmentOptions`, `TextOverflowModes`, `FontStyles`, `TextWrappingModes`, … | same names under the prefix, same members and values (`TypesetterText.TextOverflowModes.ELLIPSIS`) |
| members `fontSize`, `enableAutoSizing`, `SetText()`, `ForceMeshUpdate()`, `textInfo` | `font_size`, `enable_auto_sizing`, `set_text()`, `force_mesh_update()`, `text_info` |

Members follow Godot's snake_case convention mechanically (camelCase → snake_case, `TMP_` →
prefix, enum members UPPER_SNAKE), so a TextMeshPro call has exactly one Typesetter spelling. Where
Godot already owns a name (`Control.size`, `CanvasItem.material`, `Node.name`), the TextMeshPro
member keeps its own name if distinct (`font_material`, `font_shared_material`) or is documented as
the Godot equivalent (`rectTransform` → the Control itself). `docs/api-map.md` will list every
TextMeshPro public member, its Typesetter spelling and its status (same / renamed / adapted /
not applicable).

---

## 10. Risks and open questions

1. **TextServer glyph dictionaries.** May be too slow for large dynamic text. Mitigation: cache shaped runs; harfrust fallback behind a trait (§7.2).
2. **Layout quirks only measurable in Unity:** line-height composition, auto-size iteration, ellipsis and page rules, `<pos>`/`<space>` units. The dump harness exists to find these; expect several rounds.
3. **Shaping-feature defaults.** If Unity's TextMeshPro differs from HarfBuzz on kerning or ligature defaults for Inter, the defaults are adjusted to match, and the difference is documented.
4. **Compatibility renderer.** The shaders must stay within GLES3 (no coarse/fine derivatives; `textureGrad` fine).
5. **Web export** depends on gdext's wasm support. Desktop targets come first.
6. **Scope.** The full feature list is large. Milestones (§11) deliver a usable vertical slice early.

---

## 11. Proposed milestones for step 2

- **M0. Scaffold.** Repo, README (with provenance), MIT licence, `project.godot`, vendored GUT, Rust crate plus `native/build.py`, `.gdextension`, CI.
- **M1. Font assets.** skrifa loading, SDF/SDFAA/SDF8–32/bitmap/MSDF/MTSDF generation, packing, static and dynamic atlases, fallback chains, `TypesetterFontAsset` resource, generator API. Benchmarks for generation.
- **M2. Unity harness, layout and 2D vertical slice.** `tools/unity-dump`, first ground-truth cases, TextServer shaping, layout core, `TypesetterLabel` with the base distance-field shader, textInfo, parity comparison. Benchmarks for layout. First report of measured parity.
- **M3. Rich text and layout completeness.** Every tag, style sheets, gradients, sprites, all overflow modes, auto-size, pages, linked text, `max_visible_*`, preferred and rendered values.
- **M4. Shaders and 3D.** Full Distance Field feature set, Overlay, Mobile and Bitmap variants, `TypesetterText3D`, masking, pixel parity per feature.
- **M5. Input field and editor tooling.** `TypesetterInputField`, the Font Asset Creator dock, inspectors, menu items to create text nodes, icons.
- **M6. Downstream and polish.** `TypesetterTextLayout` measure/shape/draw API checked against godot-uxml's text cases (read-only), benchmarks against Label and Label3D, `docs/api-map.md`, manual, demo scenes.
