extends SceneTree
## Renders bench/cases/*.json with Typesetter the way tools/unity-dump renders
## them with TextMesh Pro (same render size, camera and rectangle placement),
## writing bench/godot/<case>.png. Needs a GPU (run without --headless):
##
##     godot --path . -s res://tests/render/render_cases.gd -- [--only a,b] [--out dir]
##
## Compare with `python tools/compare_renders.py`.

const CASES := "res://bench/cases/"
var out_dir := "res://bench/godot/"
var only := []
var assets := {}
var enums := {}


func _init() -> void:
	var args := OS.get_cmdline_user_args()
	for i in args.size():
		if args[i] == "--only" and i + 1 < args.size():
			only = Array(args[i + 1].split(","))
		elif args[i] == "--out" and i + 1 < args.size():
			out_dir = args[i + 1]
	enums = JSON.parse_string(FileAccess.get_file_as_string("res://bench/unity/_enums.json"))
	DirAccess.make_dir_recursive_absolute(ProjectSettings.globalize_path(out_dir))
	_run.call_deferred()


func _run() -> void:
	var files := DirAccess.get_files_at(CASES)
	var n := 0
	for f in files:
		if not f.ends_with(".json"):
			continue
		var name := f.get_basename()
		if not only.is_empty() and not only.has(name):
			continue
		var c: Dictionary = JSON.parse_string(FileAccess.get_file_as_string(CASES + f))
		await _render(name, c)
		n += 1
	print("rendered %d cases to %s" % [n, out_dir])
	quit()


func _asset(c: Dictionary) -> TypesetterFontAsset:
	var a: Dictionary = c.get("asset", {})
	var font: String = c.get("font", "Inter-Regular.ttf")
	var pt := int(a.get("pointSize", 90))
	var pad := int(a.get("padding", 9))
	var mode: String = a.get("renderMode", "SDFAA")
	var atlas: Array = a.get("atlas", [1024, 1024])
	var key := "%s_%d_%d_%s_%dx%d" % [font, pt, pad, mode, atlas[0], atlas[1]]
	if assets.has(key):
		return assets[key]
	var ff := FontFile.new()
	ff.load_dynamic_font("res://tests/fonts/" + font)
	var rm: int = {"SMOOTH": 0, "SMOOTH_HINTED": 1, "RASTER": 2, "RASTER_HINTED": 3, "SDF": 4, "SDF8": 5, "SDF16": 6, "SDF32": 7, "SDFAA": 8, "SDFAA_HINTED": 9, "MSDF": 10, "MTSDF": 11}.get(mode, 8)
	var fa := TypesetterFontAsset.create_font_asset(ff, pt, pad, rm, int(atlas[0]), int(atlas[1]), 1, true)
	if a.has("characters"):
		fa.try_add_characters(a.characters, true)
	assets[key] = fa
	return fa


static func snake(s: String) -> String:
	return s.to_snake_case()


func _enum_value(enum_name: String, v: Variant) -> Variant:
	if v is String and enums.has(enum_name) and enums[enum_name].has(v):
		return int(enums[enum_name][v])
	return v


func _apply_props(node: Node, props: Dictionary) -> void:
	for k in props:
		var v = props[k]
		match k:
			"alignment":
				node.alignment = _enum_value("TextAlignmentOptions", v)
			"textWrappingMode":
				node.text_wrapping_mode = _enum_value("TextWrappingModes", v)
			"overflowMode":
				node.overflow_mode = _enum_value("TextOverflowModes", v)
			"fontStyle":
				if v is String:
					var flags := 0
					for part in v.split(","):
						flags |= int(enums.FontStyles.get(part.strip_edges(), 0))
					node.font_style = flags
				else:
					node.font_style = int(v)
			"fontWeight":
				node.font_weight = _enum_value("FontWeight", v)
			"horizontalMapping", "verticalMapping":
				node.set(snake(k), _enum_value("TextureMappingOptions", v))
			"margin":
				node.margin = Vector4(v[0], v[1], v[2], v[3])
			"color":
				node.color = Color.html(v) if v is String else Color(v[0], v[1], v[2], v[3])
			"enableKerning":
				node.enable_kerning = v
			"enableWordWrapping":
				node.enable_word_wrapping = v
			_:
				var sk := snake(k)
				if sk in node:
					node.set(sk, v)
				else:
					push_warning("unknown property " + k)


const MATERIAL_IDS := {
	"ID_FaceColor": "face_color", "ID_FaceDilate": "face_dilate", "ID_OutlineColor": "outline_color",
	"ID_OutlineWidth": "outline_width", "ID_OutlineSoftness": "outline_softness",
	"ID_UnderlayColor": "underlay_color", "ID_UnderlayOffsetX": "underlay_offset_x", "ID_UnderlayOffsetY": "underlay_offset_y",
	"ID_UnderlayDilate": "underlay_dilate", "ID_UnderlaySoftness": "underlay_softness",
	"ID_GlowColor": "glow_color", "ID_GlowOffset": "glow_offset", "ID_GlowInner": "glow_inner", "ID_GlowOuter": "glow_outer", "ID_GlowPower": "glow_power",
	"ID_WeightNormal": "weight_normal", "ID_WeightBold": "weight_bold", "ID_Sharpness": "sharpness", "ID_PerspectiveFilter": "perspective_filter",
	"ID_LightAngle": "light_angle", "ID_Bevel": "bevel", "ID_BevelOffset": "bevel_offset", "ID_BevelWidth": "bevel_width",
	"ID_BevelRoundness": "bevel_roundness", "ID_BevelClamp": "bevel_clamp", "ID_SpecularColor": "specular_color",
	"ID_Shininess": "specular_power", "ID_Reflectivity": "reflectivity", "ID_Diffuse": "diffuse", "ID_Ambient": "ambient",
}
const KEYWORDS := {
	"Keyword_Underlay": ["underlay_type", 1], "Keyword_Glow": ["glow_enabled", true], "Keyword_Bevel": ["bevel_enabled", true],
	"UNDERLAY_ON": ["underlay_type", 1], "UNDERLAY_INNER": ["underlay_type", 2], "GLOW_ON": ["glow_enabled", true], "BEVEL_ON": ["bevel_enabled", true],
}


func _apply_material(node: Node, c: Dictionary) -> void:
	if not c.has("material") and not c.has("keywords"):
		return
	var m: ShaderMaterial = node.font_material
	for k in c.get("keywords", []):
		if KEYWORDS.has(k):
			m.set_shader_parameter(KEYWORDS[k][0], KEYWORDS[k][1])
	var mat: Dictionary = c.get("material", {})
	for k in mat:
		var pn: String = MATERIAL_IDS.get(k, snake(k.trim_prefix("_")))
		var v = mat[k]
		m.set_shader_parameter(pn, Color.html(v) if v is String else v)
	TypesetterShaderUtilities.update_shader_ratios(m)
	node.font_material = m


func _render(name: String, c: Dictionary) -> void:
	var rnd: Dictionary = c.get("render", {})
	var size: Array = rnd.get("size", [512, 256])
	var rw := int(size[0])
	var rh := int(size[1])
	var ppu := float(rnd.get("pixelsPerUnit", 10))
	var bg := Color.html(rnd.get("background", "#000000FF"))
	var rect: Array = c.get("rect", [400, 200])
	var pivot: Array = c.get("pivot", [0.5, 0.5])
	var vp := SubViewport.new()
	vp.size = Vector2i(rw, rh)
	vp.transparent_bg = false
	vp.render_target_update_mode = SubViewport.UPDATE_ALWAYS
	vp.canvas_item_default_texture_filter = Viewport.DEFAULT_CANVAS_ITEM_TEXTURE_FILTER_LINEAR
	root.add_child(vp)
	var node: Node
	if c.get("component", "ugui") == "3d":
		var env := WorldEnvironment.new()
		env.environment = Environment.new()
		env.environment.background_mode = Environment.BG_COLOR
		env.environment.background_color = bg
		env.environment.tonemap_mode = Environment.TONE_MAPPER_LINEAR
		vp.add_child(env)
		var cam := Camera3D.new()
		cam.projection = Camera3D.PROJECTION_ORTHOGONAL
		cam.size = rh / ppu
		cam.position = Vector3(0, 0, 10)
		cam.near = 0.01
		cam.far = 100
		vp.add_child(cam)
		var t := TypesetterText3D.new()
		t.rect_size = Vector2(rect[0], rect[1])
		t.pivot = Vector2(pivot[0], pivot[1])
		node = t
		vp.add_child(t)
	else:
		var bgr := ColorRect.new()
		bgr.color = bg
		bgr.size = Vector2(rw, rh)
		vp.add_child(bgr)
		var l := TypesetterLabel.new()
		l.size = Vector2(rect[0], rect[1])
		# Unity: anchored at the centre, pivot y measured upwards.
		l.position = Vector2(rw * 0.5 - rect[0] * pivot[0], rh * 0.5 - rect[1] * (1.0 - pivot[1]))
		node = l
		vp.add_child(l)
	node.font = _asset(c)
	_apply_props(node, c.get("props", {}))
	_apply_material(node, c)
	node.text = c.get("text", "")
	node.force_mesh_update(true, true)
	await process_frame
	await process_frame
	await RenderingServer.frame_post_draw
	var img := vp.get_texture().get_image()
	img.convert(Image.FORMAT_RGBA8)
	img.save_png(ProjectSettings.globalize_path(out_dir).path_join(name + ".png"))
	vp.queue_free()
	await process_frame
