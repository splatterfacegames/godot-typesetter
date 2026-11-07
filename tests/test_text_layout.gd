extends GutTest
## TypesetterTextLayout: the node-free measure / lay out / draw service.

const SAMPLE := "The quick <b>brown</b> fox jumps over the lazy dog, again and again."

var font_asset: TypesetterFontAsset


func before_all() -> void:
	var f := FontFile.new()
	f.load_dynamic_font("res://tests/fonts/Inter-Regular.ttf")
	font_asset = TypesetterFontAsset.create_font_asset(f, 48, 5)


func _style() -> TypesetterTextStyle:
	var s := TypesetterTextStyle.new()
	s.font = font_asset
	s.font_size = 24.0
	return s


func _layout() -> TypesetterTextLayout:
	var l := TypesetterTextLayout.new()
	l.set_style(_style())
	l.set_text(SAMPLE)
	return l


func _label(w: float, h: float) -> TypesetterLabel:
	var l := TypesetterLabel.new()
	l.font = font_asset
	l.font_size = 24.0
	l.size = Vector2(w, h)
	l.text = SAMPLE
	add_child_autofree(l)
	l.force_mesh_update(true)
	return l


func test_measure_matches_the_label() -> void:
	var node := _label(200, 400)
	var wrapped := _layout().measure(Vector2(200, INF))
	var p := node.get_preferred_values(200, INF)
	assert_eq(wrapped.x, 200.0, "wrapped text fills the available width")
	assert_almost_eq(wrapped.y, p.y, 0.001)
	assert_gt(_layout().measure().x, wrapped.x)


func test_measure_is_fractional() -> void:
	var w := _layout().measure().x
	assert_ne(w, roundf(w), "advances are not rounded to whole pixels")


func test_measure_leaves_the_layout_alone() -> void:
	var l := _layout()
	l.layout(Rect2(0, 0, 200, 400))
	var lines := l.text_info.line_count
	l.measure()
	assert_eq(l.text_info.line_count, lines)


func test_layout_matches_label_positions() -> void:
	var node := _label(300, 400)
	var l := _layout()
	l.layout(Rect2(10, 20, 300, 400))
	var a: TypesetterTextInfo = l.text_info
	var b: TypesetterTextInfo = node.text_info
	assert_eq(a.character_count, b.character_count)
	assert_eq(a.line_count, b.line_count)
	for i in a.character_count:
		assert_almost_eq(a.character_info[i].origin, b.character_info[i].origin, 0.001)
		assert_almost_eq(a.character_info[i].baseline, b.character_info[i].baseline, 0.001)


func test_draw_and_clear_own_their_canvas_items() -> void:
	var host := Control.new()
	add_child_autofree(host)
	var l := _layout()
	l.layout(Rect2(0, 0, 300, 200))
	l.draw(host.get_canvas_item())
	assert_eq(l.get_surface_count(), 1)
	assert_eq(l._items.size(), 1)
	var rid: RID = l._items[0]
	l.draw(host.get_canvas_item(), Transform2D(0.0, Vector2(5, 5)), Color.RED)
	assert_eq(l._items[0], rid, "redrawing reuses the item")
	l.clear()
	assert_eq(l._items.size(), 0)


func test_outline_gets_its_own_material() -> void:
	var l := _layout()
	var s := l.get_style()
	s.outline_width = 0.2
	s.outline_color = Color.RED
	s.underlay_enabled = true
	s.underlay_offset_x = 0.5
	var host := Control.new()
	add_child_autofree(host)
	l.layout(Rect2(0, 0, 300, 200))
	l.draw(host.get_canvas_item())
	var mat: ShaderMaterial = l._surfaces[0].material_used
	assert_almost_eq(float(mat.get_shader_parameter(&"outline_width")), 0.2, 0.0001)
	assert_ne(mat, TypesetterMaterialManager.get_default_material(font_asset))
	l.clear()


func test_changes_invalidate() -> void:
	var l := _layout()
	l.layout(Rect2(0, 0, 300, 200))
	var n := l.text_info.character_count
	l.set_text("short")
	assert_ne(l.text_info.character_count, n)
	assert_true(l.get_rendered_size().x > 0.0)
	var m := l.measure()
	var big := _style()
	big.font_size = 48.0
	l.set_style(big)
	assert_gt(l.measure().x, m.x)


func test_no_font_is_harmless() -> void:
	var l := TypesetterTextLayout.new()
	l.set_text("x")
	assert_eq(l.measure(), Vector2.ZERO)
	l.layout(Rect2(0, 0, 10, 10))
	l.draw(RID())
	assert_eq(l.get_surface_count(), 0)
