extends GutTest
## TypesetterLabel / TypesetterText3D: API surface, dirty tracking, meshes,
## materials, events.

var font_asset: TypesetterFontAsset


func before_all() -> void:
	var f := FontFile.new()
	f.load_dynamic_font("res://tests/fonts/Inter-Regular.ttf")
	font_asset = TypesetterFontAsset.create_font_asset(f, 48, 5)


func _label(t: String = "Hello world") -> TypesetterLabel:
	var l := TypesetterLabel.new()
	l.font = font_asset
	l.size = Vector2(300, 100)
	l.text = t
	add_child_autofree(l)
	l.force_mesh_update(true)
	return l


func test_label_layout_and_surfaces() -> void:
	var l := _label()
	assert_eq(l.text_info.character_count, 11)
	assert_eq(l.text_info.line_count, 1)
	assert_gt(l.preferred_width, 100.0)
	assert_eq(l._ts_surfaces.size(), 1)
	assert_not_null(l.mesh)


func test_alignment_round_trip() -> void:
	var l := _label()
	l.alignment = TypesetterLabel.TextAlignmentOptions.BOTTOM_RIGHT
	assert_eq(l.horizontal_alignment, TypesetterLabel.HorizontalAlignmentOptions.RIGHT)
	assert_eq(l.vertical_alignment, TypesetterLabel.VerticalAlignmentOptions.BOTTOM)
	assert_eq(l.alignment, 0x404)


func test_set_text_formats() -> void:
	var l := _label()
	l.set_text("{0} / {1:2} / {2:00}", 7, 3.14159, 5)
	assert_eq(l.text, "7 / 3.14 / 05")


func test_text_changed_signal() -> void:
	var l := _label()
	watch_signals(l)
	l.text = "changed"
	assert_signal_emitted(l, "text_changed")


func test_parsed_text_strips_tags() -> void:
	var l := _label("<b>bold</b> <color=red>red</color>")
	assert_eq(l.get_parsed_text(), "bold red")


func test_get_preferred_values_forms() -> void:
	var l := _label("abc")
	var a: Vector2 = l.get_preferred_values()
	var b: Vector2 = l.get_preferred_values("abc")
	assert_almost_eq(a.x, b.x, 0.001)
	var c: Vector2 = l.get_preferred_values("a long line of text that wraps", 100, 0)
	assert_gt(c.y, a.y)


func test_font_material_instance_is_separate() -> void:
	var l := _label()
	var shared := l.font_shared_material if l.font_shared_material else TypesetterMaterialManager.get_default_material(font_asset)
	l.outline_width = 0.2
	assert_ne(l.font_material, shared)
	assert_almost_eq(float(l.font_material.get_shader_parameter(&"outline_width")), 0.2, 1e-6)
	assert_ne(shared.get_shader_parameter(&"outline_width"), 0.2)


func test_max_visible_characters() -> void:
	var l := _label("abcdefgh")
	l.max_visible_characters = 3
	l.force_mesh_update(true)
	var vis := 0
	for i in l.text_info.character_count:
		if l.text_info.character_info[i].is_visible:
			vis += 1
	assert_eq(vis, 3)


func test_overflow_reports() -> void:
	var l := _label("one two three four five six seven eight nine ten eleven twelve")
	l.size = Vector2(80, 60)
	l.overflow_mode = TypesetterLabel.TextOverflowModes.TRUNCATE
	l.force_mesh_update(true)
	assert_true(l.is_text_overflowing)
	assert_true(l.is_text_truncated)
	assert_gt(l.first_overflow_character_index, 0)


func test_linked_overflow() -> void:
	var a := _label("one two three four five six seven eight nine ten eleven twelve")
	a.size = Vector2(80, 60)
	var b := _label("")
	b.size = Vector2(80, 200)
	a.overflow_mode = TypesetterLabel.TextOverflowModes.LINKED
	a.linked_text_component = b
	a.force_mesh_update(true)
	b.force_mesh_update(true)
	var first := a.first_overflow_character_index
	assert_gt(first, 0)
	assert_false(b.text_info.character_info[first - 1].is_visible, "before the link point is hidden")
	assert_true(b.text_info.character_info[first].is_visible, "the link point starts here")


func test_missing_character_event() -> void:
	watch_signals(TypesetterSettings.events)
	var l := _label("x一y")
	assert_signal_emitted(TypesetterSettings.events, "missing_character")


func test_text3d_mesh_and_scale() -> void:
	var t := TypesetterText3D.new()
	t.font = font_asset
	t.text = "Hello 3D"
	add_child_autofree(t)
	t.force_mesh_update(true)
	assert_eq(t._ts_mesh.get_surface_count(), 1)
	assert_almost_eq(t.font_scale, 36.0 / 48.0 * 0.1, 1e-5)
	assert_gt(t.get_aabb().size.x, 1.0)
	assert_not_null(t._ts_mesh.surface_get_material(0))


func test_geometry_sorting_reverse() -> void:
	var l := _label("ab")
	var m1: ArrayMesh = l.mesh
	var first_x: float = m1.surface_get_arrays(0)[Mesh.ARRAY_VERTEX][0].x
	l.geometry_sorting_order = TypesetterLabel.VertexSortingOrder.REVERSE
	l.force_mesh_update(true)
	var m2: ArrayMesh = l.mesh
	var rev_x: float = m2.surface_get_arrays(0)[Mesh.ARRAY_VERTEX][0].x
	assert_gt(rev_x, first_x)


func test_text_style_and_style_sheet() -> void:
	var l := _label("Title")
	l.style_sheet = TypesetterStyleSheet.create_default()
	l.text_style = l.style_sheet.get_style("H1")
	l.force_mesh_update(true)
	assert_eq(l.text_info.character_count, 5)
	assert_eq(l.text_info.character_info[0].index, 0)
	assert_true(l.is_using_bold)


func test_hit_testing() -> void:
	var l := _label("Hello world")
	var c: TypesetterCharacterInfo = l.text_info.character_info[1]
	var p := Vector2((c.origin + c.x_advance) * 0.5, (c.ascender + c.descender) * 0.5)
	assert_eq(l.get_text_core().find_intersecting_character(p, true), 1)


func test_dont_render() -> void:
	var l := _label()
	l.render_mode = TypesetterLabel.TextRenderFlags.DONT_RENDER
	l.force_mesh_update(true)
	assert_eq(l._ts_surfaces.size(), 0)
	assert_eq(l.text_info.character_count, 11)


func test_font_material_edits_reach_the_drawn_material() -> void:
	var l := _label("Edit me")
	var m := l.font_material as ShaderMaterial
	l.force_mesh_update(true)
	# Edited in place: 2D draws with the instance itself.
	m.set_shader_parameter(&"outline_width", 0.4)
	var used := l._ts_surfaces[0].material_used as ShaderMaterial
	assert_almost_eq(float(used.get_shader_parameter(&"outline_width")), 0.4, 0.0001)
	# Edited and reassigned (TMP's fontMaterial = m): the same in 3D.
	var t := TypesetterText3D.new()
	t.font = font_asset
	t.text = "Edit me"
	add_child_autofree(t)
	t.force_mesh_update(true)
	var m3 := t.font_material as ShaderMaterial
	m3.set_shader_parameter(&"outline_width", 0.3)
	t.font_material = m3
	t.force_mesh_update(true)
	var used3 := t._ts_surfaces[0].material_used as ShaderMaterial
	assert_almost_eq(float(used3.get_shader_parameter(&"outline_width")), 0.3, 0.0001)
