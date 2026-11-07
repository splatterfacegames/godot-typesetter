extends GutTest
## Layout parity of the shipped nodes against TextMesh Pro: every case in
## bench/cases is laid out by TypesetterLabel / TypesetterText3D and compared
## with Unity's dump in bench/unity (character origins and advances, line
## count, preferred and rendered values, overflow).

const Case := preload("res://tests/support/typesetter_case.gd")
const TOL := 0.02
## Same list and reasons as KNOWN_GAPS in native/examples/parity.rs; see docs/parity.md.
const KNOWN_GAPS := [
	"rt_escapes", "tp_missing_glyph", "rt_unknown", "tp_prefctx_2", "tp_prefctx_8", "tp_prefctx_9",
	"tp_open_bad_values", "tp_italic_sizes",
]

var runner


func before_all() -> void:
	runner = Case.new()


func _origin_offset(c: Dictionary, node: Node) -> Vector2:
	# Unity reports x relative to the pivot; TypesetterLabel uses the top-left.
	if node is TypesetterText3D:
		return Vector2.ZERO
	var rect: Array = c.get("rect", [400, 200])
	var pivot: Array = c.get("pivot", [0.5, 0.5])
	return Vector2(-rect[0] * pivot[0], 0)


func _check_case(name: String) -> void:
	var c := Case.load_case(name)
	var u := Case.load_unity(name)
	if u.is_empty():
		pending("no Unity dump for " + name)
		return
	var node: Node = runner.build(c)
	add_child_autofree(node)
	node.force_mesh_update(true, true)
	var info: TypesetterTextInfo = node.text_info
	var off := _origin_offset(c, node)
	assert_eq(info.character_count, int(u.characterCount), name + ": character count")
	assert_eq(info.line_count, int(u.lineCount), name + ": line count")
	var bad := 0
	var n := mini(info.character_count, int(u.characterCount))
	for i in n:
		var a: TypesetterCharacterInfo = info.character_info[i]
		var b: Dictionary = u.characters[i]
		if not bool(b.isVisible) and not a.is_visible:
			continue
		# After </cspace>, TMP reports xAdvance from the line's left edge rather than
		# in rectangle space, so that value carries no origin offset.
		var adv_ok := absf(a.x_advance + off.x - float(b.xAdvance)) <= TOL or absf(a.x_advance - float(b.xAdvance)) <= TOL
		if absf(a.origin + off.x - float(b.origin)) > TOL or not adv_ok:
			bad += 1
			if bad <= 3:
				gut.p("%s char %d '%s': origin %.3f/%.3f adv %.3f/%.3f" % [name, i, a.character, a.origin + off.x, b.origin, a.x_advance + off.x, b.xAdvance])
	assert_eq(bad, 0, name + ": characters off by more than %.2f" % TOL)
	assert_almost_eq(node.preferred_width, float(u.preferredWidth), 0.05, name + ": preferred width")
	assert_almost_eq(node.preferred_height, float(u.preferredHeight), 0.05, name + ": preferred height")
	assert_eq(node.is_text_overflowing, bool(u.isTextOverflowing), name + ": overflowing")


func test_all_cases() -> void:
	for name in Case.case_names():
		if not name in KNOWN_GAPS:
			_check_case(name)
