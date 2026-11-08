extends SceneTree
## Checks the in-place 2D mesh update against a full rebuild, byte for byte.
## Needs a real renderer (run without --headless):
##
##     godot --path . -s res://tests/render/verify_mesh_update.gd

const PAIRS := [["Score 123", "Score 987"], ["<color=red>ab</color>c", "x<b>y</b>z"], ["Wrapped text that runs on", "Different words, same size"]]


func _init() -> void:
	_run.call_deferred()


func _surface(l: TypesetterLabel) -> Dictionary:
	var m: ArrayMesh = l._ts_meshes[0]
	return RenderingServer.mesh_get_surface(m.get_rid(), 0)


func _label(asset: TypesetterFontAsset, t: String) -> TypesetterLabel:
	var l := TypesetterLabel.new()
	l.font = asset
	l.font_size = 30
	l.size = Vector2(200, 200)
	root.add_child(l)
	l.text = t
	l.force_mesh_update(true)
	return l


func _text3d(asset: TypesetterFontAsset, t: String) -> TypesetterText3D:
	var n := TypesetterText3D.new()
	n.font = asset
	n.font_size = 30
	root.add_child(n)
	n.text = t
	n.force_mesh_update(true)
	return n


func _counts(n: TypesetterText3D) -> Array:
	var out := []
	for i in n._ts_mesh.get_surface_count():
		out.append(n._ts_mesh.surface_get_array_len(i))
	return out


func _run() -> void:
	var ff := FontFile.new()
	ff.load_dynamic_font("res://tests/fonts/Inter-Regular.ttf")
	var asset := TypesetterFontAsset.create_font_asset(ff, 90, 9)
	var failed := 0
	for pair in PAIRS:
		var a := _label(asset, pair[0])
		var count_before: int = (a._ts_meshes[0] as ArrayMesh).surface_get_array_len(0)
		a.text = pair[1]
		a.force_mesh_update(true)
		var b := _label(asset, pair[1])
		var count_after: int = (b._ts_meshes[0] as ArrayMesh).surface_get_array_len(0)
		var sa := _surface(a)
		var sb := _surface(b)
		var same: bool = sa.vertex_data == sb.vertex_data and sa.attribute_data == sb.attribute_data
		var path := "in place" if count_before == count_after else "rebuilt"
		print("%-40s %-9s %s" % ["%s -> %s" % pair, path, "same" if same else "DIFFERENT"])
		if not same:
			failed += 1
			var va: PackedByteArray = sa.attribute_data
			var vb: PackedByteArray = sb.attribute_data
			for k in mini(va.size(), vb.size()):
				if va[k] != vb[k]:
					print("  first attribute difference at byte %d: %d vs %d" % [k, va[k], vb[k]])
					break
	for pair in PAIRS:
		var a3 := _text3d(asset, pair[0])
		var before := _counts(a3)
		a3.text = pair[1]
		a3.force_mesh_update(true)
		var b3 := _text3d(asset, pair[1])
		var same3 := _counts(a3) == _counts(b3)
		for i in mini(a3._ts_mesh.get_surface_count(), b3._ts_mesh.get_surface_count()):
			var sa := RenderingServer.mesh_get_surface(a3._ts_mesh.get_rid(), i)
			var sb := RenderingServer.mesh_get_surface(b3._ts_mesh.get_rid(), i)
			same3 = same3 and sa.vertex_data == sb.vertex_data and sa.attribute_data == sb.attribute_data
		var path3 := "in place" if before == _counts(b3) else "rebuilt"
		print("3D %-37s %-9s %s" % ["%s -> %s" % pair, path3, "same" if same3 else "DIFFERENT"])
		if not same3:
			failed += 1
	print("FAILED %d" % failed if failed else "ALL SAME")
	quit(1 if failed else 0)
