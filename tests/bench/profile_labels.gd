extends SceneTree
## 100 short labels whose text changes: where the time goes outside a frame.
##
##     godot --path . -s res://tests/bench/profile_labels.gd

const N := 100
const FRAMES := 60


func _init() -> void:
	_run.call_deferred()


func _run() -> void:
	var ff := FontFile.new()
	ff.load_dynamic_font("res://tests/fonts/Inter-Regular.ttf")
	var asset := TypesetterFontAsset.create_font_asset(ff, 90, 9)
	var nodes: Array[TypesetterLabel] = []
	for i in N:
		var x := TypesetterLabel.new()
		x.font = asset
		x.font_size = 18
		x.size = Vector2(200, 30)
		root.add_child(x)
		nodes.append(x)
	await process_frame
	var rows := {}
	var t := 0
	var t_set := 0
	var t_update := 0
	for f in FRAMES:
		t = Time.get_ticks_usec()
		for i in N:
			nodes[i].text = "Score %d · %d" % [f * 31 + i, i]
		t_set += Time.get_ticks_usec() - t
		t = Time.get_ticks_usec()
		for i in N:
			nodes[i]._ts_deferred_update()
		t_update += Time.get_ticks_usec() - t
		await process_frame
	rows["set text, 100 labels"] = t_set / 1000.0 / FRAMES
	rows["update, 100 labels"] = t_update / 1000.0 / FRAMES
	var l := nodes[0]
	var phases := {"layout": 0, "meshes only": 0, "surfaces": 0, "present": 0}
	for f in FRAMES:
		for i in N:
			var x := nodes[i]
			x.text = "Score %d · %d" % [f * 17 + i, i]
			t = Time.get_ticks_usec()
			x._ts_layout()
			phases.layout += Time.get_ticks_usec() - t
			t = Time.get_ticks_usec()
			x._ts_build_meshes_impl()
			phases["meshes only"] += Time.get_ticks_usec() - t
			t = Time.get_ticks_usec()
			x._ts_build_surfaces()
			phases.surfaces += Time.get_ticks_usec() - t
			t = Time.get_ticks_usec()
			x._ts_present()
			phases.present += Time.get_ticks_usec() - t
		await process_frame
	for k in phases:
		rows["  %s, 100 labels" % k] = phases[k] / 1000.0 / FRAMES
	for k in rows:
		print("%-40s %8.3f ms" % [k, rows[k]])
	quit()
