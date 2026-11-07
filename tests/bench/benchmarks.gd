extends SceneTree
## Typesetter benchmarks. Writes bench/results/<date>.json and prints a table.
##
##     godot --path . -s res://tests/bench/benchmarks.gd [-- --quick]
##
## * atlas: time to generate an ASCII atlas at 90 pt per render mode.
## * layout: TypesetterTextCore.update() per 1 000 characters (plain, rich, wrapped).
## * mesh: layout + mesh build per 1 000 characters.
## * frame: per-frame cost of N labels whose text changes every frame,
##   compared with Godot's Label and Label3D (needs a window for GPU timing;
##   headless measures CPU only).

var quick := false
var results := {}
var font: FontFile
var asset: TypesetterFontAsset

const LOREM := "The quick brown fox jumps over the lazy dog. Pack my box with five dozen liquor jugs! Sphinx of black quartz, judge my vow; how vexingly quick daft zebras jump. "


func _init() -> void:
	quick = "--quick" in OS.get_cmdline_user_args()
	font = FontFile.new()
	font.load_dynamic_font("res://tests/fonts/Inter-Regular.ttf")
	_run.call_deferred()


func _ms(f: Callable, reps: int) -> float:
	f.call()
	var t := Time.get_ticks_usec()
	for i in reps:
		f.call()
	return (Time.get_ticks_usec() - t) / 1000.0 / reps


func _run() -> void:
	var ascii := ""
	for c in range(32, 127):
		ascii += char(c)
	# --- atlas generation
	var modes := {"SMOOTH": 0, "RASTER": 2, "SDF": 4, "SDF8": 5, "SDF16": 6, "SDF32": 7, "SDFAA": 8, "MSDF": 10, "MTSDF": 11}
	var atlas := {}
	for m in modes:
		if quick and m in ["SDF16", "SDF32"]:
			continue
		var t := Time.get_ticks_usec()
		var a := TypesetterFontAsset.create_font_asset(font, 90, 9, modes[m], 1024, 1024, 1, true)
		a.try_add_characters(ascii)
		atlas[m] = (Time.get_ticks_usec() - t) / 1000.0
	results["atlas_ascii_90pt_ms"] = atlas
	asset = TypesetterFontAsset.create_font_asset(font, 90, 9)
	asset.try_add_characters(ascii + "…")
	# --- layout per 1k characters
	var plain := LOREM.repeat(7).substr(0, 1000)
	var rich := ""
	var words := plain.split(" ")
	for i in words.size():
		rich += ("<b>%s</b> " if i % 3 == 0 else ("<color=#ff8040>%s</color> " if i % 3 == 1 else "%s ")) % words[i]
	var core := TypesetterTextCore.new()
	core.font = asset
	core.font_size = 24
	core.rect_size = Vector2(100000, 1000)
	core.text_wrapping_mode = 0
	core.text = plain
	var reps := 20 if quick else 100
	var layout := {}
	layout["plain_nowrap"] = _ms(core.update, reps)
	core.rect_size = Vector2(600, 100000)
	core.text_wrapping_mode = 1
	layout["plain_wrapped"] = _ms(core.update, reps)
	core.text = rich
	layout["rich_wrapped"] = _ms(core.update, reps)
	results["layout_ms_per_1k_chars"] = layout
	# --- layout + mesh
	core.text = plain
	var meshes: Array[ArrayMesh] = []
	results["layout_and_mesh_ms_per_1k_chars"] = _ms(func():
		core.update()
		core.build_meshes(meshes), reps)
	# --- comparison: Godot Label text change + shaping (CPU)
	var label := Label.new()
	label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	label.size = Vector2(600, 100000)
	label.add_theme_font_size_override("font_size", 24)
	root.add_child(label)
	var flip := [false]
	results["godot_label_ms_per_1k_chars"] = _ms(func():
		flip[0] = not flip[0]
		label.text = plain if flip[0] else plain + " "
		label.get_minimum_size()
		label.get_line_count(), reps)
	var tl := TypesetterLabel.new()
	tl.font = asset
	tl.font_size = 24
	tl.size = Vector2(600, 100000)
	root.add_child(tl)
	results["typesetter_label_ms_per_1k_chars"] = _ms(func():
		flip[0] = not flip[0]
		tl.text = plain if flip[0] else plain + " "
		tl.force_mesh_update(true), reps)
	var l3 := Label3D.new()
	l3.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	l3.width = 600
	l3.font_size = 24
	root.add_child(l3)
	results["godot_label3d_ms_per_1k_chars"] = _ms(func():
		flip[0] = not flip[0]
		l3.text = plain if flip[0] else plain + " "
		l3.get_aabb(), reps)
	var t3 := TypesetterText3D.new()
	t3.font = asset
	t3.font_size = 24
	t3.rect_size = Vector2(60, 10000)
	root.add_child(t3)
	results["typesetter_text3d_ms_per_1k_chars"] = _ms(func():
		flip[0] = not flip[0]
		t3.text = plain if flip[0] else plain + " "
		t3.force_mesh_update(true), reps)
	# --- per-frame: 100 short labels changing every frame
	var n := 100
	var nodes: Array = []
	for i in n:
		var x := TypesetterLabel.new()
		x.font = asset
		x.font_size = 18
		x.size = Vector2(200, 30)
		root.add_child(x)
		nodes.append(x)
	var frames := 30 if quick else 120
	var t0 := Time.get_ticks_usec()
	for f in frames:
		for i in n:
			nodes[i].text = "Score %d · %d" % [f * 31 + i, i]
		await process_frame
	results["frame_ms_100_typesetter_labels_changing"] = (Time.get_ticks_usec() - t0) / 1000.0 / frames
	for x in nodes:
		x.queue_free()
	nodes.clear()
	for i in n:
		var x := Label.new()
		x.size = Vector2(200, 30)
		x.add_theme_font_size_override("font_size", 18)
		root.add_child(x)
		nodes.append(x)
	t0 = Time.get_ticks_usec()
	for f in frames:
		for i in n:
			nodes[i].text = "Score %d · %d" % [f * 31 + i, i]
		await process_frame
	results["frame_ms_100_godot_labels_changing"] = (Time.get_ticks_usec() - t0) / 1000.0 / frames
	# --- report
	results["godot"] = Engine.get_version_info().string
	results["renderer"] = RenderingServer.get_current_rendering_method()
	results["cpu"] = OS.get_processor_name()
	print(JSON.stringify(results, "  "))
	DirAccess.make_dir_recursive_absolute(ProjectSettings.globalize_path("res://bench/results"))
	var path := "res://bench/results/%s.json" % Time.get_datetime_string_from_system().replace(":", "-")
	var fa := FileAccess.open(path, FileAccess.WRITE)
	fa.store_string(JSON.stringify(results, "  "))
	print("wrote ", path)
	quit()
