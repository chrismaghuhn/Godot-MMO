# Spieler (#13): Laufen und Springen mit Kollision aus dem Core (TerraforgeBody), Fliegen zum
# Testen ohne Kollision. Kamera in Augenhöhe (First-Person, D-020).
#
# WASD laufen, Leertaste springen, Shift rennen, F zwischen Laufen und Fliegen wechseln, im Flug
# Q/E runter/hoch. Rechte Maustaste gedrückt halten zum Umsehen.
#
# Graben und Bauen (#14) wie im Viewer: Linksklick gräbt, B baut, T wechselt das Werkzeug, X fällt
# den anvisierten Baum. Gebaut wird aus dem Inventar (#18), vom gewählten Platz `slot` (Auswahl in
# `inventory.gd`).
# Ziel ist der Punkt unter dem Fadenkreuz, höchstens 8 m entfernt; prüfen und ausführen tut es der
# Server (`TerraforgeTerrain.dig`/`build`).
#
# Der Körper bewegt sich im Physik-Takt (interpoliert). Die Kamera ist ein eigener Node ohne
# Interpolation, der in `_process` der interpolierten Position folgt und den Blickwinkel direkt aus
# der Maus nimmt: So reagiert die Maus sofort statt mit einem Physik-Takt Verzögerung.
extends Node3D

const FLY_SPEED := 32.0
const FLY_RUN_SPEED := 180.0
const MOUSE_SENSITIVITY := 0.003

var terrain: TerraforgeTerrain
var body: TerraforgeBody
var camera: Camera3D
var flying := false
## Blickrichtung (Radiant): yaw um die Hochachse (0 = −z), pitch nach oben positiv.
var yaw := 0.0
var pitch := 0.0
## Werkzeug-Stufe; wie im Viewer zu Beginn die stärkste.
var tool := TerraforgeTerrain.tool_count() - 1
## Inventarplatz (feste Nummer ab 0), aus dem gebaut wird.
var slot := 0
## Inventar offen: Klicks graben dann nicht.
var ui_open := false
## Ergebnis der letzten Grab- oder Bau-Aktion (für das Overlay).
var edit_message := ""


## Fußpunkt `feet`, Blick auf `target`.
func setup(world_terrain: TerraforgeTerrain, feet: Vector3, target: Vector3) -> void:
	terrain = world_terrain
	body = TerraforgeBody.create(feet)
	position = feet
	camera = Camera3D.new()
	camera.top_level = true
	camera.physics_interpolation_mode = Node.PHYSICS_INTERPOLATION_MODE_OFF
	add_child(camera)
	var direction := target - (feet + Vector3.UP * body.eye_height())
	yaw = atan2(-direction.x, -direction.z)
	pitch = atan2(direction.y, Vector2(direction.x, direction.z).length())
	reset_physics_interpolation()
	_place_camera()


## Fester Blickwinkel (Radiant), z. B. für Screenshots.
func look(new_yaw: float, new_pitch: float) -> void:
	yaw = new_yaw
	pitch = new_pitch
	_place_camera()


func set_flying(enabled: bool) -> void:
	if flying and not enabled:
		body.set_position(position)
	flying = enabled


func _unhandled_input(event: InputEvent) -> void:
	if event is InputEventMouseButton and event.button_index == MOUSE_BUTTON_RIGHT:
		Input.mouse_mode = Input.MOUSE_MODE_CAPTURED if event.pressed else Input.MOUSE_MODE_VISIBLE
	elif event is InputEventMouseMotion and Input.mouse_mode == Input.MOUSE_MODE_CAPTURED:
		yaw -= event.relative.x * MOUSE_SENSITIVITY
		pitch = clampf(pitch - event.relative.y * MOUSE_SENSITIVITY, -1.5, 1.5)
	elif event is InputEventMouseButton and event.button_index == MOUSE_BUTTON_LEFT and event.pressed and not ui_open:
		dig()
	elif event is InputEventKey and event.pressed and not event.echo:
		match event.physical_keycode:
			KEY_F:
				set_flying(not flying)
			KEY_B:
				build()
			KEY_T:
				tool = (tool + 1) % TerraforgeTerrain.tool_count()
			KEY_X:
				terrain.fell(camera.global_position, -camera.global_basis.z)


## Gräbt unter dem Fadenkreuz; das Ergebnis kommt einige Frames später (`edit_message`).
func dig() -> int:
	return terrain.dig(camera.global_position, -camera.global_basis.z, tool)


func build() -> int:
	return terrain.build(camera.global_position, -camera.global_basis.z, slot)


static func axis(negative: Key, positive: Key) -> float:
	return float(Input.is_physical_key_pressed(positive)) - float(Input.is_physical_key_pressed(negative))


func _physics_process(delta: float) -> void:
	if body == null:
		return
	var run := Input.is_physical_key_pressed(KEY_SHIFT)
	if flying:
		var input := Vector3(axis(KEY_A, KEY_D), axis(KEY_Q, KEY_E), axis(KEY_W, KEY_S))
		if input != Vector3.ZERO:
			# Q/E senkrecht in der Welt, WASD in Blickrichtung (mit Neigung der Kamera).
			var direction := camera.global_basis * Vector3(input.x, 0, input.z) + Vector3(0, input.y, 0)
			position += direction.normalized() * (FLY_RUN_SPEED if run else FLY_SPEED) * delta
		return
	var movement := Vector2(axis(KEY_A, KEY_D), axis(KEY_S, KEY_W))
	body.step(terrain, movement, yaw, Input.is_physical_key_pressed(KEY_SPACE), run, delta)
	position = body.get_position()


func _process(_delta: float) -> void:
	_place_camera()
	if terrain != null:
		for result in terrain.take_edit_results():
			edit_message = result.message


func _place_camera() -> void:
	if camera == null or body == null:
		return
	var feet := get_global_transform_interpolated().origin if is_inside_tree() else position
	camera.global_transform = Transform3D(Basis.from_euler(Vector3(pitch, yaw, 0.0)), feet + Vector3.UP * body.eye_height())
