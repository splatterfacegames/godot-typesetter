extends SceneTree
## Where a TypesetterLabel text change spends its time, phase by phase.
##
##     godot --headless --path . -s res://tests/bench/profile_update.gd [-- <characters>]

const REPS := 200


func _init() -> void:
	_run.call_deferred()


func _ms(f: Callable) -> float:
	f.call()
	var t := Time.get_ticks_usec()
	for i in REPS:
		f.call()
	return (Time.get_ticks_usec() - t) / 1000.0 / REPS


func _run() -> void:
	var ff := FontFile.new()
	ff.load_dynamic_font("res://tests/fonts/Inter-Regular.ttf")
	var asset := TypesetterFontAsset.create_font_asset(ff, 90, 9)
	var n := 1000
	for a in OS.get_cmdline_user_args():
		if a.is_valid_int():
			n = int(a)
	var plain := ""
	while plain.length() < n:
		plain += "The quick brown fox jumps over the lazy dog. "
	plain = plain.substr(0, n)
	var l := TypesetterLabel.new()
	l.font = asset
	l.font_size = 24
	l.size = Vector2(600, 10000)
	root.add_child(l)
	l.text = plain
	l.force_mesh_update(true)
	var flip := [false]
	var core: TypesetterTextCore = l._ts_core
	var rows := {}
	rows["whole force_mesh_update"] = _ms(func():
		flip[0] = not flip[0]
		l.text = plain if flip[0] else plain + " "
		l.force_mesh_update(true))
	rows["  text setter alone"] = _ms(func():
		flip[0] = not flip[0]
		l.text = plain if flip[0] else plain + " ")
	rows["  force_mesh_update, text unchanged"] = _ms(func(): l.force_mesh_update(true))
	rows["  _ts_update(true)"] = _ms(func(): l._ts_update(true))
	rows["  _ts_layout"] = _ms(func(): l._ts_layout())
	rows["  _ts_source_text"] = _ms(func(): l._ts_source_text())
	rows["  _ts_present"] = _ms(func(): l._ts_present())
	rows["  sync settings into core"] = _ms(func(): l._ts_sync_core(core, plain))
	rows["  core.update (layout)"] = _ms(func(): core.update())
	rows["  after layout"] = _ms(func(): l._ts_after_layout())
	rows["  build surfaces (meshes + materials)"] = _ms(func(): l._ts_build_surfaces())
	rows["    core.build_meshes"] = _ms(func():
		var arr: Array[ArrayMesh] = []
		arr.assign(l._ts_meshes)
		core.build_meshes(arr))
	rows["  get_text_info (pre_render_text)"] = _ms(func():
		core.update()
		core.get_text_info())
	rows["    core.update alone, for the line above"] = _ms(func(): core.update())
	rows["  present"] = _ms(func(): l._ts_present())
	rows["  TypesetterTextUtilities lookups"] = _ms(func():
		TypesetterTextUtilities.font_lookup()
		TypesetterTextUtilities.material_lookup()
		TypesetterTextUtilities.gradient_lookup())
	rows["  TypesetterSettings fallbacks + breaking"] = _ms(func():
		TypesetterSettings.get_fallback_font_assets()
		TypesetterSettings.get_leading_characters()
		TypesetterSettings.get_following_characters()
		TypesetterSettings.get_missing_glyph_character())
	rows["  shader padding"] = _ms(func(): TypesetterShaderUtilities.get_padding(l._ts_base_material(), false, false))
	rows["  style sheet lookup"] = _ms(func():
		var sheet := TypesetterSettings.get_default_style_sheet()
		if sheet != null:
			sheet.to_lookup())
	for k in rows:
		print("%-44s %8.3f ms" % [k, rows[k]])
	quit()
