extends GutTest
## TypesetterInputField: editing, validation, selection, signals.

var field: TypesetterInputField


func before_each() -> void:
	field = TypesetterInputField.new()
	field.size = Vector2(200, 30)
	add_child_autofree(field)
	await wait_process_frames(1)


func _key(code: Key, unicode: int = 0, ctrl := false, shift := false) -> void:
	var e := InputEventKey.new()
	e.pressed = true
	e.keycode = code
	e.unicode = unicode
	e.ctrl_pressed = ctrl
	e.shift_pressed = shift
	field._key(e)


func _type(s: String) -> void:
	for ch in s:
		_key(KEY_NONE, ch.unicode_at(0))


func test_typing_and_value_changed() -> void:
	watch_signals(field)
	_type("hello")
	assert_eq(field.text, "hello")
	assert_signal_emit_count(field, "value_changed", 5)
	assert_eq(field.string_position, 5)


func test_backspace_and_word_delete() -> void:
	_type("hello world")
	_key(KEY_BACKSPACE)
	assert_eq(field.text, "hello worl")
	_key(KEY_BACKSPACE, 0, true)
	assert_eq(field.text, "hello ")


func test_selection_and_replace() -> void:
	_type("hello world")
	field.select_all()
	assert_eq(field.get_selected_text(), "hello world")
	_type("x")
	assert_eq(field.text, "x")


func test_integer_validation() -> void:
	field.content_type = TypesetterInputField.ContentType.INTEGER_NUMBER
	_type("-12a3.4-")
	assert_eq(field.text, "-1234")


func test_decimal_validation() -> void:
	field.content_type = TypesetterInputField.ContentType.DECIMAL_NUMBER
	_type("3.1.4x")
	assert_eq(field.text, "3.14")


func test_name_validation_capitalises() -> void:
	field.content_type = TypesetterInputField.ContentType.NAME
	_type("jane doe")
	assert_eq(field.text, "Jane Doe")


func test_email_validation() -> void:
	field.content_type = TypesetterInputField.ContentType.EMAIL_ADDRESS
	_type("a@@b..c")
	assert_eq(field.text, "a@b.c")


func test_character_limit() -> void:
	field.character_limit = 3
	_type("abcdef")
	assert_eq(field.text, "abc")


func test_password_masks_display() -> void:
	field.content_type = TypesetterInputField.ContentType.PASSWORD
	_type("secret")
	assert_eq(field.text, "secret")
	assert_eq(field.text_component.text, "******")


func test_submit_and_newline() -> void:
	watch_signals(field)
	_type("a")
	_key(KEY_ENTER)
	assert_signal_emitted(field, "submit")
	field.line_type = TypesetterInputField.LineType.MULTI_LINE_NEWLINE
	field.grab_focus()
	field.string_position = field.text.length()
	_key(KEY_ENTER)
	assert_eq(field.text, "a\n")


func test_undo_redo() -> void:
	_type("ab")
	_key(KEY_Z, 0, true)
	assert_eq(field.text, "a")
	_key(KEY_Y, 0, true)
	assert_eq(field.text, "ab")


func test_custom_validator() -> void:
	field.character_validation = TypesetterInputField.CharacterValidation.CUSTOM_VALIDATOR
	field.input_validator = TypesetterInputValidator.DigitValidator.new()
	_type("a1b2")
	assert_eq(field.text, "12")


func test_set_text_without_notify() -> void:
	watch_signals(field)
	field.set_text_without_notify("quiet")
	assert_eq(field.text, "quiet")
	assert_signal_not_emitted(field, "value_changed")
