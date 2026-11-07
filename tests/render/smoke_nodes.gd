extends SceneTree
func _init():
	for f in DirAccess.get_files_at("res://addons/typesetter/runtime"):
		if f.ends_with(".gd"):
			var s = load("res://addons/typesetter/runtime/" + f)
			if s == null or not s.can_instantiate():
				print("FAIL ", f)
	var l = TypesetterLabel.new()
	root.add_child(l)
	l.size = Vector2(300, 100)
	l.text = "Hello <b>bold</b> <color=red>red</color> world"
	l.force_mesh_update(true)
	print("pref ", l.preferred_width, " ", l.preferred_height, " surfaces ", l._ts_surfaces.size(), " chars ", l.text_info.character_count)
	print(l.get_parsed_text(), " | ", l.get_preferred_values("abc"), " | ", l.alignment)
	l.set_text("Score {0} time {1:2}", 42, 3.14159)
	print(l.text)
	var t = TypesetterText3D.new()
	root.add_child(t)
	t.text = "Hello 3D"
	t.force_mesh_update(true)
	print("3d surfaces ", t._ts_mesh.get_surface_count(), " aabb ", t._ts_mesh.get_aabb(), " mat ", t._ts_mesh.surface_get_material(0))
	print(t.font_scale, " ", t.bounds)
	quit()
