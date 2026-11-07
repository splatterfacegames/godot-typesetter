@tool
extends EditorPlugin
## Typesetter editor plugin: registers project settings and editor tooling.


func _enter_tree() -> void:
	TypesetterSettings.register()


func _exit_tree() -> void:
	pass
