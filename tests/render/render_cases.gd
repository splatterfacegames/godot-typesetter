extends SceneTree
## Renders bench/cases/*.json with Typesetter the way tools/unity-dump renders
## them with TextMesh Pro (same render size, camera and rectangle placement),
## writing bench/godot/<case>.png. Needs a GPU (run without --headless):
##
##     godot --path . -s res://tests/render/render_cases.gd -- [--only a,b] [--out dir]
##
## Compare with `python tools/compare_renders.py`.

const CASES := "res://bench/cases/"
var out_dir := "res://bench/godot/"
var only := []
var runner = preload("res://tests/support/typesetter_case.gd").new()


func _init() -> void:
	var args := OS.get_cmdline_user_args()
	for i in args.size():
		if args[i] == "--only" and i + 1 < args.size():
			only = Array(args[i + 1].split(","))
		elif args[i] == "--param" and i + 1 < args.size():
			var kv := args[i + 1].split("=")
			runner.params[kv[0]] = float(kv[1])
		elif args[i] == "--out" and i + 1 < args.size():
			out_dir = args[i + 1]
	DirAccess.make_dir_recursive_absolute(ProjectSettings.globalize_path(out_dir))
	_run.call_deferred()


func _run() -> void:
	var files := DirAccess.get_files_at(CASES)
	var n := 0
	for f in files:
		if not f.ends_with(".json"):
			continue
		var name := f.get_basename()
		if not only.is_empty() and not only.has(name):
			continue
		var c: Dictionary = JSON.parse_string(FileAccess.get_file_as_string(CASES + f))
		await _render(name, c)
		n += 1
	print("rendered %d cases to %s" % [n, out_dir])
	quit()


func _render(name: String, c: Dictionary) -> void:
	var rnd: Dictionary = c.get("render", {})
	var size: Array = rnd.get("size", [512, 256])
	var rw := int(size[0])
	var rh := int(size[1])
	var ppu := float(rnd.get("pixelsPerUnit", 10))
	var bg := Color.html(rnd.get("background", "#000000FF"))
	var rect: Array = c.get("rect", [400, 200])
	var pivot: Array = c.get("pivot", [0.5, 0.5])
	var vp := SubViewport.new()
	vp.size = Vector2i(rw, rh)
	vp.transparent_bg = false
	vp.render_target_update_mode = SubViewport.UPDATE_ALWAYS
	vp.canvas_item_default_texture_filter = Viewport.DEFAULT_CANVAS_ITEM_TEXTURE_FILTER_LINEAR
	root.add_child(vp)
	var node: Node
	if c.get("component", "ugui") == "3d":
		var env := WorldEnvironment.new()
		env.environment = Environment.new()
		env.environment.background_mode = Environment.BG_COLOR
		env.environment.background_color = bg
		env.environment.tonemap_mode = Environment.TONE_MAPPER_LINEAR
		vp.add_child(env)
		var cam := Camera3D.new()
		cam.projection = Camera3D.PROJECTION_ORTHOGONAL
		cam.size = rh / ppu
		cam.position = Vector3(0, 0, 10)
		cam.near = 0.01
		cam.far = 100
		vp.add_child(cam)
		node = runner.build(c)
		vp.add_child(node)
	else:
		var bgr := ColorRect.new()
		bgr.color = bg
		bgr.size = Vector2(rw, rh)
		vp.add_child(bgr)
		node = runner.build(c)
		# Unity: anchored at the centre, pivot y measured upwards.
		node.position = Vector2(rw * 0.5 - rect[0] * pivot[0], rh * 0.5 - rect[1] * (1.0 - pivot[1]))
		vp.add_child(node)
	node.force_mesh_update(true, true)
	await process_frame
	await process_frame
	await RenderingServer.frame_post_draw
	var img := vp.get_texture().get_image()
	img.convert(Image.FORMAT_RGBA8)
	img.save_png(ProjectSettings.globalize_path(out_dir).path_join(name + ".png"))
	vp.queue_free()
	await process_frame
