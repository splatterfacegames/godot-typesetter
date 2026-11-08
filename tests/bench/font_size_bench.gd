extends SceneTree
## What a runtime font-size change costs, and whether it touches the atlas.
##
##     godot --path . -s res://tests/bench/font_size_bench.gd
##
## 100 short labels whose font size changes every frame (Typesetter and Label), the
## same with auto-size on, and a check that the atlas neither grows nor rebuilds.

const N := 100
const FRAMES := 120


func _init() -> void:
	_run.call_deferred()


func _frames(nodes: Array, step: Callable) -> float:
	var t0 := Time.get_ticks_usec()
	for f in FRAMES:
		for i in nodes.size():
			step.call(nodes[i], f, i)
		await process_frame
	return (Time.get_ticks_usec() - t0) / 1000.0 / FRAMES


func _run() -> void:
	DisplayServer.window_set_vsync_mode(DisplayServer.VSYNC_DISABLED)
	Engine.max_fps = 0
	var ff := load("res://demo/fonts/Inter-Regular.ttf") as FontFile
	var asset := TypesetterFontAsset.create_font_asset(ff, 90, 9)
	var rebuilds := [0]
	asset.atlas_updated.connect(func() -> void: rebuilds[0] += 1)
	var results := {}
	var t_empty := Time.get_ticks_usec()
	for f in FRAMES:
		await process_frame
	var empty := (Time.get_ticks_usec() - t_empty) / 1000.0 / FRAMES

	var labels: Array = []
	for i in N:
		var l := TypesetterLabel.new()
		l.font = asset
		l.size = Vector2(240, 60)
		l.text = "Score %d" % i
		l.position = Vector2((i % 10) * 120, (i / 10) * 70)
		root.add_child(l)
		labels.append(l)
	await process_frame
	var glyphs_before: int = asset.get_character_table().size()
	var textures_before := asset.atlas_textures.size()
	rebuilds[0] = 0
	results["typesetter: font size changing, 100 labels"] = await _frames(labels, func(l, f, i): l.font_size = 14.0 + fmod(f * 0.37 + i, 30.0)) - empty
	results["typesetter: text changing, 100 labels"] = await _frames(labels, func(l, f, i): l.text = "Score %d" % (f * 31 + i)) - empty
	for l in labels:
		l.enable_auto_sizing = true
		l.font_size_min = 8
		l.font_size_max = 48
	results["typesetter: auto-size on, text changing"] = await _frames(labels, func(l, f, i): l.text = "Score %d" % (f * 31 + i)) - empty
	results["typesetter: auto-size on, box size changing"] = await _frames(labels, func(l, f, i): l.size = Vector2(120.0 + fmod(f * 2.0 + i, 120.0), 60)) - empty
	var glyphs_after: int = asset.get_character_table().size()
	for l in labels:
		l.queue_free()

	var godot: Array = []
	for i in N:
		var l := Label.new()
		l.size = Vector2(240, 60)
		l.text = "Score %d" % i
		l.position = Vector2((i % 10) * 120, (i / 10) * 70)
		root.add_child(l)
		godot.append(l)
	await process_frame
	results["Label: font size changing, 100 labels"] = await _frames(godot, func(l, f, i): l.add_theme_font_size_override("font_size", int(14.0 + fmod(f * 0.37 + i, 30.0)))) - empty
	results["Label: text changing, 100 labels"] = await _frames(godot, func(l, f, i): l.text = "Score %d" % (f * 31 + i)) - empty

	for k in results:
		print("%-48s %7.3f ms per frame" % [k, results[k]])
	print("atlas_updated during the size tests: %d" % rebuilds[0])
	print("atlas textures before/after: %d / %d" % [textures_before, asset.atlas_textures.size()])
	print("glyphs before/after: %s / %s" % [glyphs_before, glyphs_after])
	quit()
