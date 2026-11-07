extends GutTest
## TypesetterFontAsset: creation, dynamic population, atlas modes, features.

var font: FontFile


func before_all() -> void:
	font = FontFile.new()
	font.load_dynamic_font("res://tests/fonts/Inter-Regular.ttf")


func test_create_defaults() -> void:
	var a := TypesetterFontAsset.create_font_asset(font)
	assert_not_null(a)
	assert_eq(a.atlas_render_mode, TypesetterFontAsset.SDFAA)
	assert_eq(a.atlas_padding, 9)
	assert_eq(a.get_gradient_scale(), 10.0)
	assert_eq(a.get_sampling_point_size(), 90.0)
	assert_almost_eq(a.face_info.ascent_line, 87.2, 0.5)


func test_dynamic_add_characters() -> void:
	var a := TypesetterFontAsset.create_font_asset(font, 48, 5, TypesetterFontAsset.SDFAA, 512, 512)
	assert_true(a.try_add_characters("Hello"))
	assert_true(a.has_character("H".unicode_at(0)))
	assert_false(a.has_character("Z".unicode_at(0)))
	assert_eq(a.get_atlas_texture_count(), 1)
	var missing := a.try_add_characters_get_missing("あ")
	assert_eq(missing, "あ", "Inter has no kana")


func test_every_render_mode_builds() -> void:
	for mode in range(12):
		var a := TypesetterFontAsset.create_font_asset(font, 32, 4, mode, 256, 256)
		assert_true(a.try_add_characters("Ag@"), "mode %d" % mode)
		var tex := a.get_atlas_texture()
		assert_not_null(tex, "mode %d texture" % mode)
		var img := tex.get_image()
		var fmt_ok := img.get_format() == (Image.FORMAT_RGBA8 if mode >= 10 else Image.FORMAT_R8)
		assert_true(fmt_ok, "mode %d format" % mode)


func test_hinted_metrics_are_whole_pixels() -> void:
	var a := TypesetterFontAsset.create_font_asset(font, 48, 5, TypesetterFontAsset.SMOOTH_HINTED, 512, 512)
	a.try_add_characters("lHo")
	for g in a.get_glyph_table():
		var m: Dictionary = g.metrics
		assert_eq(fmod(m.width, 1.0), 0.0)
		assert_eq(fmod(m.horizontal_advance, 1.0), 0.0)


func test_runtime_features_use_legacy_kern_only() -> void:
	# TMP's runtime TryAddCharacters ignores GPOS kerning; the creator reads it.
	var a := TypesetterFontAsset.create_font_asset(font, 48, 5)
	a.try_add_characters("AVTo", true)
	assert_eq(a.get_font_feature_table().glyph_pair_adjustment_records.size(), 0)
	a.update_font_feature_table()
	assert_gt(a.get_font_feature_table().glyph_pair_adjustment_records.size(), 0)


func test_static_asset_does_not_grow() -> void:
	var a := TypesetterFontAsset.create_font_asset(font, 32, 4)
	a.try_add_characters("abc")
	a.atlas_population_mode = TypesetterFontAsset.STATIC
	var core := TypesetterTextCore.new()
	core.font = a
	core.text = "abcxyz"
	core.update()
	assert_false(a.has_character("x".unicode_at(0)))


func test_save_and_reload() -> void:
	var a := TypesetterFontAsset.create_font_asset(font, 32, 4)
	a.try_add_characters("Save me")
	var path := "user://test_font_asset.tres"
	assert_eq(ResourceSaver.save(a, path), OK)
	var b: TypesetterFontAsset = ResourceLoader.load(path, "", ResourceLoader.CACHE_MODE_IGNORE)
	assert_not_null(b)
	assert_true(b.has_character("S".unicode_at(0)))
	assert_eq(b.get_glyph_table().size(), a.get_glyph_table().size())


func test_auto_point_size() -> void:
	var a := TypesetterFontAsset.create_font_asset(font, 90, 5, TypesetterFontAsset.SDFAA, 256, 256)
	var ascii := ""
	for c in range(32, 127):
		ascii += char(c)
	var pt := a.find_auto_point_size(ascii)
	assert_between(pt, 10, 60)
