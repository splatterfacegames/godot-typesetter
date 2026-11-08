extends Control
## The Typesetter web demo: every piece of text on this screen is a Typesetter
## node rendered with its distance-field shaders.

const INK := Color("0a1550")
const PAPER := Color("f2f5ff")
const BLUE := Color("1238c9")
const FACE := Color("ffe43a")
const EDGE := Color("ff3e8a")

var asset: TypesetterFontAsset
var title: TypesetterLabel
var wave: TypesetterLabel
var spinner: TypesetterText3D
var sliders := {}
var _time := 0.0
## `-- --reel`: drive the sliders and a zoom on a fixed timeline (for recording).
var _reel := "--reel" in OS.get_cmdline_user_args()


func _ready() -> void:
	var ff := FontFile.new()
	ff.load_dynamic_font("res://demo/fonts/Inter-Regular.ttf")
	asset = TypesetterFontAsset.create_font_asset(ff, 90, 9)
	_build()


func _build() -> void:
	var bg := ColorRect.new()
	bg.color = BLUE
	add_child(bg)
	bg.set_anchors_and_offsets_preset(PRESET_FULL_RECT)

	var margin := MarginContainer.new()
	for side in ["left", "right"]:
		margin.add_theme_constant_override("margin_" + side, 40)
	for side in ["top", "bottom"]:
		margin.add_theme_constant_override("margin_" + side, 20)
	add_child(margin)
	margin.set_anchors_and_offsets_preset(PRESET_FULL_RECT)
	var root := VBoxContainer.new()
	root.add_theme_constant_override("separation", 12)
	margin.add_child(root)

	title = _label("Typesetter", 120)
	title.custom_minimum_size = Vector2(0, 170)
	title.horizontal_alignment = TypesetterLabel.HorizontalAlignmentOptions.CENTER
	title.color = FACE
	var m := title.font_material as ShaderMaterial
	m.set_shader_parameter(&"outline_color", EDGE)
	m.set_shader_parameter(&"underlay_type", 1)
	m.set_shader_parameter(&"underlay_color", Color(INK, 0.85))
	m.set_shader_parameter(&"glow_color", Color(EDGE, 0.6))
	root.add_child(title)

	var controls := HFlowContainer.new()
	controls.add_theme_constant_override("h_separation", 24)
	root.add_child(controls)
	for spec in [["Dilate", &"face_dilate", -0.4, 0.6, 0.0], ["Softness", &"outline_softness", 0.0, 0.6, 0.0],
			["Outline", &"outline_width", 0.0, 0.6, 0.15], ["Underlay", &"underlay_offset_x", 0.0, 1.0, 0.5],
			["Glow", &"glow_outer", 0.0, 1.0, 0.0]]:
		controls.add_child(_slider(spec[0], spec[1], spec[2], spec[3], spec[4]))
	_apply_material()

	var rich := _label(
		"<b>Bold</b>, <i>italic</i>, <u>underlined</u>, <s>struck</s>, <mark=#ffe43a60>marked</mark>, "
		+ "<size=150%>bigger</size>, <sup>super</sup>and<sub>sub</sub>, <color=#ff3e8a>colour</color>, "
		+ "<cspace=6>spaced</cspace> and <smallcaps>Small Caps</smallcaps>. "
		+ "Every piece of text here is a Typesetter node.", 26)
	rich.color = PAPER
	rich.custom_minimum_size = Vector2(0, 110)
	rich.text_wrapping_mode = TypesetterLabel.TextWrappingModes.NORMAL
	root.add_child(rich)

	wave = _label("Per-character animation through text_info, as TMP scripts do it.", 30)
	wave.color = FACE
	wave.custom_minimum_size = Vector2(0, 60)
	root.add_child(wave)

	var view := SubViewportContainer.new()
	view.stretch = true
	view.size_flags_vertical = SIZE_EXPAND_FILL
	view.custom_minimum_size = Vector2(0, 160)
	root.add_child(view)
	var vp := SubViewport.new()
	vp.transparent_bg = true
	view.add_child(vp)
	var cam := Camera3D.new()
	cam.position = Vector3(0, 0, 6)
	vp.add_child(cam)
	spinner = TypesetterText3D.new()
	spinner.font = asset
	spinner.font_size = 9
	spinner.rect_size = Vector2(80, 12)
	spinner.horizontal_alignment = TypesetterText3D.HorizontalAlignmentOptions.CENTER
	spinner.vertical_alignment = TypesetterText3D.VerticalAlignmentOptions.MIDDLE
	spinner.color = PAPER
	spinner.text = "Sharp at any angle"
	vp.add_child(spinner)


func _label(t: String, size: float) -> TypesetterLabel:
	var l := TypesetterLabel.new()
	l.font = asset
	l.font_size = size
	l.text = t
	return l


func _slider(caption: String, param: StringName, lo: float, hi: float, value: float) -> Control:
	var box := VBoxContainer.new()
	box.custom_minimum_size = Vector2(150, 0)
	var name_label := _label(caption, 18)
	name_label.color = PAPER
	name_label.custom_minimum_size = Vector2(0, 24)
	box.add_child(name_label)
	var s := HSlider.new()
	s.min_value = lo
	s.max_value = hi
	s.step = 0.01
	s.value = value
	s.value_changed.connect(func(_v: float) -> void: _apply_material())
	box.add_child(s)
	sliders[param] = s
	return box


func _apply_material() -> void:
	var m := title.font_material as ShaderMaterial
	for param in sliders:
		var v: float = sliders[param].value
		m.set_shader_parameter(param, v)
		if param == &"underlay_offset_x":
			m.set_shader_parameter(&"underlay_offset_y", -v)
		if param == &"glow_outer":
			m.set_shader_parameter(&"glow_enabled", v > 0.0)
	TypesetterShaderUtilities.update_shader_ratios(m)
	title.font_material = m


func _process(delta: float) -> void:
	_time += delta
	if _reel:
		_run_reel(_time)
	if spinner:
		spinner.rotation = Vector3(0.0, sin(_time * 0.7) * 1.1, sin(_time * 0.4) * 0.15)
	if wave == null:
		return
	# The TMP pattern: lay out once, then move each character's vertices.
	wave.force_mesh_update()
	var info := wave.text_info
	var meshes := info.mesh_info
	if meshes.is_empty():
		return
	var verts: PackedVector3Array = meshes[0].vertices
	for i in info.character_count:
		var c: TypesetterCharacterInfo = info.character_info[i]
		if not c.is_visible:
			continue
		var dy := sin(_time * 4.0 + i * 0.35) * 6.0
		for k in 4:
			var vi := c.vertex_index + k
			if vi < verts.size():
				verts[vi].y += dy
	meshes[0].vertices = verts
	wave.update_vertex_data()


## Keyframes per slider: [time, value] pairs, eased between.
const REEL := {
	&"outline_width": [[0.0, 0.15], [2.0, 0.15], [3.5, 0.55], [5.0, 0.15], [15.0, 0.15]],
	&"underlay_offset_x": [[0.0, 0.5], [5.0, 0.5], [6.5, 1.0], [7.5, 0.0], [8.5, 0.5], [15.0, 0.5]],
	&"outline_softness": [[0.0, 0.0], [6.0, 0.0], [7.0, 0.45], [8.5, 0.0], [15.0, 0.0]],
	&"glow_outer": [[0.0, 0.0], [8.5, 0.0], [9.5, 0.8], [11.0, 0.8], [11.8, 0.0], [15.0, 0.0]],
	&"face_dilate": [[0.0, 0.0], [9.0, 0.0], [9.8, 0.35], [10.6, -0.25], [11.4, 0.0], [15.0, 0.0]],
}


func _keyframe(keys: Array, t: float) -> float:
	for i in keys.size() - 1:
		var a: Array = keys[i]
		var b: Array = keys[i + 1]
		if t <= b[0]:
			var k := clampf((t - a[0]) / maxf(b[0] - a[0], 1e-4), 0.0, 1.0)
			return lerpf(a[1], b[1], k * k * (3.0 - 2.0 * k))
	return keys[-1][1]


func _run_reel(t: float) -> void:
	for param in REEL:
		sliders[param].set_value_no_signal(_keyframe(REEL[param], t))
	_apply_material()
	# Zoom into the title to show the edges stay sharp, then back out.
	var z := _keyframe([[0.0, 1.0], [11.8, 1.0], [13.0, 5.5], [14.0, 5.5], [14.9, 1.0]], t)
	title.pivot_offset = Vector2(title.size.x * 0.5, title.size.y * 0.55)
	title.scale = Vector2(z, z)
	title.z_index = 1 if z > 1.001 else 0
