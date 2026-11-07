extends GutTest
## TypesetterFaceInfo must report what TextMesh Pro reports for the same font.
## Layout parity in native/examples/parity.rs reads Unity's face info directly,
## so this suite is what ties the generator to it.

const FIELDS := {
	"pointSize": "point_size", "ascentLine": "ascent_line", "descentLine": "descent_line",
	"lineHeight": "line_height", "capLine": "cap_line", "meanLine": "mean_line",
	"superscriptOffset": "superscript_offset", "superscriptSize": "superscript_size",
	"subscriptOffset": "subscript_offset", "subscriptSize": "subscript_size",
	"underlineOffset": "underline_offset", "underlineThickness": "underline_thickness",
	"strikethroughOffset": "strikethrough_offset", "strikethroughThickness": "strikethrough_thickness",
	"tabWidth": "tab_width",
}


func _check(font_file: String, dump: String) -> void:
	var f := FontFile.new()
	f.load_dynamic_font("res://tests/fonts/" + font_file)
	var asset := TypesetterFontAsset.create_font_asset(f, 90, 9)
	var truth: Dictionary = JSON.parse_string(FileAccess.get_file_as_string("res://bench/unity/atlases/" + dump)).faceInfo
	for unity_name in FIELDS:
		var ours := float(asset.face_info.get(FIELDS[unity_name]))
		assert_almost_eq(ours, float(truth[unity_name]), 0.01, "%s %s" % [font_file, unity_name])


func test_inter_matches_unity() -> void:
	_check("Inter-Regular.ttf", "Inter-Regular_90_9_SDFAA_1024x1024.json")


func test_noto_matches_unity() -> void:
	_check("NotoSansJP-Subset.ttf", "NotoSansJP-Subset_90_9_SDFAA_1024x1024.json")
