# Flug-Kamera zum Testen (bis #13 Laufen kommt): WASD + Q/E, Shift schnell, rechte Maustaste
# gedrückt halten zum Umsehen. Geschwindigkeiten wie im Viewer.
extends Camera3D

const WALK_SPEED := 32.0
const RUN_SPEED := 180.0
const MOUSE_SENSITIVITY := 0.003


func _unhandled_input(event: InputEvent) -> void:
	if event is InputEventMouseButton and event.button_index == MOUSE_BUTTON_RIGHT:
		Input.mouse_mode = Input.MOUSE_MODE_CAPTURED if event.pressed else Input.MOUSE_MODE_VISIBLE
	elif event is InputEventMouseMotion and Input.mouse_mode == Input.MOUSE_MODE_CAPTURED:
		rotation.y -= event.relative.x * MOUSE_SENSITIVITY
		rotation.x = clampf(rotation.x - event.relative.y * MOUSE_SENSITIVITY, -1.5, 1.5)


func _process(delta: float) -> void:
	var input := Vector3(
		float(Input.is_physical_key_pressed(KEY_D)) - float(Input.is_physical_key_pressed(KEY_A)),
		float(Input.is_physical_key_pressed(KEY_E)) - float(Input.is_physical_key_pressed(KEY_Q)),
		float(Input.is_physical_key_pressed(KEY_S)) - float(Input.is_physical_key_pressed(KEY_W)))
	if input == Vector3.ZERO:
		return
	var speed := RUN_SPEED if Input.is_physical_key_pressed(KEY_SHIFT) else WALK_SPEED
	# Q/E senkrecht in der Welt, WASD in Blickrichtung.
	var direction := global_basis * Vector3(input.x, 0, input.z) + Vector3(0, input.y, 0)
	position += direction.normalized() * speed * delta
