extends RefCounted
## Builds Typesetter nodes from bench/cases/*.json the way tools/unity-dump
## builds TextMesh Pro components (shared by the render harness and tests).

var assets := {}
var enums := {}
var params := {}
var _style_sheet: TypesetterStyleSheet


func _init() -> void:
	enums = JSON.parse_string(FileAccess.get_file_as_string("res://bench/unity/_enums.json"))


static func load_case(name: String) -> Dictionary:
	var c: Dictionary = JSON.parse_string(FileAccess.get_file_as_string("res://bench/cases/%s.json" % name))
	c["_name"] = name
	return c


static func load_unity(name: String) -> Dictionary:
	var p := "res://bench/unity/%s.json" % name
	if not FileAccess.file_exists(p):
		return {}
	return JSON.parse_string(FileAccess.get_file_as_string(p))


static func case_names() -> PackedStringArray:
	var out := PackedStringArray()
	for f in DirAccess.get_files_at("res://bench/cases/"):
		if f.ends_with(".json"):
			out.append(f.get_basename())
	return out


## A configured TypesetterLabel or TypesetterText3D (not in the tree).
func build(c: Dictionary) -> Node:
	var rect: Array = c.get("rect", [400, 200])
	var pivot: Array = c.get("pivot", [0.5, 0.5])
	var node: Node
	if c.get("component", "ugui") == "3d":
		var t := TypesetterText3D.new()
		t.rect_size = Vector2(rect[0], rect[1])
		t.pivot = Vector2(pivot[0], pivot[1])
		node = t
	else:
		var l := TypesetterLabel.new()
		l.size = Vector2(rect[0], rect[1])
		node = l
	node.font = _asset(c)
	_apply_case_resources(node, c)
	_apply_props(node, c.get("props", {}))
	_apply_material(node, c)
	node.text = c.get("text", "")
	return node


## Named fonts, fallbacks, gradients, sprites and styles, set up the way
## tools/unity-dump sets them up for TextMesh Pro.
func _apply_case_resources(node: Node, c: Dictionary) -> void:
	var fonts := {}
	var named: Dictionary = c.get("fonts", {})
	for alias in named:
		fonts[alias] = _asset(named[alias])
	TypesetterTextUtilities.set_lookup_override("font", fonts)
	var fallbacks: Array[TypesetterFontAsset] = []
	for alias in c.get("fallbacks", []):
		fallbacks.append(fonts[alias])
	(node.font as TypesetterFontAsset).fallback_font_asset_table = fallbacks
	var gradients := {}
	var named_gradients: Dictionary = c.get("gradients", {})
	for gradient_name in named_gradients:
		var colors := PackedColorArray()
		for html in named_gradients[gradient_name]:
			colors.append(Color.html(html))
		gradients[gradient_name] = colors
	TypesetterTextUtilities.set_lookup_override("gradient", gradients)
	node.style_sheet = _unity_style_sheet()
	if c.has("spriteAsset"):
		node.sprite_asset = _sprite_asset(c)


func _unity_style_sheet() -> TypesetterStyleSheet:
	if _style_sheet != null:
		return _style_sheet
	_style_sheet = TypesetterStyleSheet.new()
	var settings: Dictionary = JSON.parse_string(FileAccess.get_file_as_string("res://bench/unity/_settings.json"))
	var styles: Array[TypesetterStyle] = []
	for s in settings.get("styles", []):
		styles.append(TypesetterStyle.create(s.name, s.open, s.close))
	_style_sheet.styles = styles
	return _style_sheet


func _sprite_asset(c: Dictionary) -> TypesetterSpriteAsset:
	var spec: Dictionary = c.spriteAsset
	var dumped: Dictionary = load_unity(_case_name(c)).get("spriteAsset", {})
	var a := TypesetterSpriteAsset.new()
	a.resource_name = spec.name
	# The sheet the Unity harness drew and saved, when it has been dumped.
	var image := Image.load_from_file("res://bench/unity/atlases/sprites.png")
	if image == null or image.is_empty():
		image = Image.create(int(spec.width), int(spec.height), false, Image.FORMAT_RGBA8)
		image.fill(Color.WHITE)
	a.sprite_sheet = ImageTexture.create_from_image(image)
	var face: Dictionary = spec.get("faceInfo", {})
	a.face_info = {
		"point_size": float(face.get("pointSize", 0.0)), "ascent_line": float(face.get("ascentLine", 0.0)),
		"descent_line": float(face.get("descentLine", 0.0)), "line_height": float(face.get("lineHeight", 0.0)),
		"baseline": 0.0, "scale": float(face.get("scale", 1.0)),
	}
	# Unity's upgrade of the harness's legacy asset decides which glyph each
	# sprite really uses, so the dump is the reference for that mapping.
	var glyphs: Array[TypesetterSpriteGlyph] = []
	var chars: Array[TypesetterSpriteCharacter] = []
	var sprites: Array = spec.sprites
	for i in sprites.size():
		var sp: Dictionary = sprites[i]
		var dumped_char: Dictionary = dumped.characters[i] if dumped.has("characters") else {}
		var src_index := int(dumped_char.get("glyphIndex", i))
		var m: Array = dumped.glyphs[src_index].metrics if dumped.has("glyphs") else sp.metrics
		var r: Array = dumped.glyphs[src_index].rect if dumped.has("glyphs") else sp.rect
		var g := TypesetterSpriteGlyph.new()
		g.index = i
		g.glyph_rect = Rect2i(int(r[0]), int(spec.height) - int(r[1]) - int(r[3]), int(r[2]), int(r[3]))
		g.metrics_width = m[0]
		g.metrics_height = m[1]
		g.bearing_x = m[2]
		g.bearing_y = m[3]
		g.advance = m[4]
		glyphs.append(g)
		var ch := TypesetterSpriteCharacter.new()
		ch.name = sp.name
		ch.unicode = int(dumped_char.get("unicode", sp.get("unicode", 0xFFFE)))
		ch.glyph_index = i
		ch.scale = float(dumped_char.get("scale", sp.get("scale", 1.0)))
		chars.append(ch)
	a.sprite_glyph_table = glyphs
	a.sprite_character_table = chars
	return a


static func _case_name(c: Dictionary) -> String:
	return c.get("_name", "")


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
	"ID_LightAngle": "light_angle", "ID_BevelAmount": "bevel", "ID_BevelOffset": "bevel_offset", "ID_BevelWidth": "bevel_width",
	"ID_BevelRoundness": "bevel_roundness", "ID_BevelClamp": "bevel_clamp", "ID_SpecularColor": "specular_color",
	"ID_Shininess": "specular_power", "ID_Reflectivity": "reflectivity", "ID_Diffuse": "diffuse", "ID_Ambient": "ambient",
}
const KEYWORDS := {
	"Keyword_Underlay": ["underlay_type", 1], "Keyword_Glow": ["glow_enabled", true], "Keyword_Bevel": ["bevel_enabled", true],
	"UNDERLAY_ON": ["underlay_type", 1], "UNDERLAY_INNER": ["underlay_type", 2], "GLOW_ON": ["glow_enabled", true], "BEVEL_ON": ["bevel_enabled", true],
}


func _apply_material(node: Node, c: Dictionary) -> void:
	if not c.has("material") and not c.has("keywords") and params.is_empty():
		return
	var m: ShaderMaterial = node.font_material
	for k in params:
		if k == "bold_style":
			node.font.bold_style = params[k]
		else:
			m.set_shader_parameter(k, params[k])
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


