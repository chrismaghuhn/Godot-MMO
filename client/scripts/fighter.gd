# Kampf (#62, D-023): Kampfmodus, Ausholen mit Richtung, Schlag, Erholung, Block, Waffe in der Hand.
#
# R zieht die Waffe bzw. steckt sie weg. Mit gezogener Waffe ist die Maus gefangen und dreht die
# Kamera; linke Taste halten und die Maus bewegen wählt die Richtung (hoch: von oben, zur Seite: von
# dieser Seite, runter: Stich), loslassen schlägt. Wer zu früh loslässt, schlägt, sobald ganz
# ausgeholt ist. Rechte Taste blockt (gegen die Puppe noch ohne Wirkung). C wechselt die Waffe.
#
# Die Lage der Waffe kommt aus dem Core (`TerraforgeCombat.pose`), dieselbe Bahn, mit der der Server
# trifft. Den Schlag selbst rechnet der Server (`TerraforgeTerrain.attack`).
extends Node

## So weit (Pixel) muss sich die Maus beim Ausholen bewegen, um die Richtung zu wählen; sonst gilt
## nach `CHOOSE_TIME` „von oben“.
const CHOOSE_DISTANCE := 12.0
const CHOOSE_TIME := 0.15

var terrain: TerraforgeTerrain
var player: Node3D
var dummies: Node
var active := false
var weapon := 0
var info: Dictionary = {}
var weapon_node: MeshInstance3D
var hud: Label
## "ruhe", "ausholen", "schlag", "erholung", "block"
var state := "ruhe"
var direction := 0
var chosen := false
var mouse := Vector2.ZERO
var timer := 0.0
var release := false
## Ausdauer: vom Server bestätigt, dazwischen geschätzt (Werte aus dem Core).
var stamina_info: Dictionary = TerraforgeCombat.stamina_info()
var stamina: float = stamina_info.max
## Je laufendem Schlag (Nummer von `attack`): Auge, rechts und Richtung beim Loslassen, für das
## Kippen der Puppe, auch wenn die Antwort erst nach dem nächsten Ausholen kommt.
var pending := {}
var message := ""
var swings := 0
## Nur für Schnappschüsse: Waffe fest in dieser Lage halten (`TerraforgeCombat.pose`, s), sonst NAN.
var hold := NAN


func _ready() -> void:
	weapon_node = MeshInstance3D.new()
	weapon_node.top_level = true
	weapon_node.visible = false
	var material := StandardMaterial3D.new()
	material.vertex_color_use_as_albedo = true
	material.vertex_color_is_srgb = true
	material.roughness = 0.5
	weapon_node.material_override = material
	add_child(weapon_node)
	hud = Label.new()
	hud.add_theme_font_size_override("font_size", 15)
	var background := StyleBoxFlat.new()
	background.bg_color = Color(0.02, 0.03, 0.05, 0.72)
	background.set_content_margin_all(6)
	hud.add_theme_stylebox_override("normal", background)
	hud.set_anchors_and_offsets_preset(Control.PRESET_BOTTOM_LEFT)
	hud.grow_vertical = Control.GROW_DIRECTION_BEGIN
	hud.position += Vector2(10, -50)
	hud.visible = false
	var layer := CanvasLayer.new()
	layer.add_child(hud)
	add_child(layer)
	_set_weapon(0)


func _set_weapon(index: int) -> void:
	weapon = index
	info = TerraforgeCombat.weapon_info(weapon)
	weapon_node.mesh = TerraforgeCombat.weapon_mesh(weapon)


func set_active(enabled: bool) -> void:
	active = enabled
	player.combat = enabled
	weapon_node.visible = enabled
	hud.visible = enabled
	state = "ruhe"
	Input.mouse_mode = Input.MOUSE_MODE_CAPTURED if enabled else Input.MOUSE_MODE_VISIBLE


func _unhandled_input(event: InputEvent) -> void:
	if event is InputEventKey and event.pressed and not event.echo:
		if event.physical_keycode == KEY_R:
			set_active(not active)
		elif active and event.physical_keycode == KEY_C and state == "ruhe":
			_set_weapon((weapon + 1) % TerraforgeCombat.weapon_count())
	# Mit offenem Inventar schlagen Klicks nicht (die Maus bedient dann das Inventar).
	if not active or player.ui_open:
		return
	if event is InputEventMouseButton:
		if event.button_index == MOUSE_BUTTON_LEFT:
			if event.pressed:
				start_windup()
			else:
				release = true
		elif event.button_index == MOUSE_BUTTON_RIGHT:
			if event.pressed and state == "ruhe":
				state = "block"
			elif not event.pressed and state == "block":
				state = "ruhe"
	elif event is InputEventMouseMotion and state == "ausholen" and not chosen:
		mouse += event.relative
		if mouse.length() > CHOOSE_DISTANCE:
			direction = TerraforgeCombat.direction_from_mouse(mouse.x, mouse.y)
			chosen = true


## Ausholen beginnen (auch für Tests).
func start_windup() -> void:
	if state != "ruhe":
		return
	state = "ausholen"
	timer = 0.0
	mouse = Vector2.ZERO
	chosen = false
	release = false
	direction = 0


func _strike_time() -> float:
	return info.strike_thrust if direction == 3 else info.strike


func _process(delta: float) -> void:
	stamina = minf(stamina_info.max, stamina + stamina_info.regen * delta)
	for result: Dictionary in terrain.take_attack_results():
		message = result.message
		var swing: Array = pending.get(result.request, [])
		pending.erase(result.request)
		if result.ok:
			stamina = result.stamina
			if result.has("hit") and dummies != null and not swing.is_empty():
				dummies.hit(result.hit, swing[0], swing[1], swing[2])
	if not active:
		return
	var bar := "█".repeat(int(stamina / 10.0)) + "░".repeat(10 - int(stamina / 10.0))
	var aim: String = TerraforgeCombat.direction_name(direction) if state == "ausholen" and chosen else ""
	hud.text = "%s   Ausdauer %s %3.0f   %s %s\n[LMB halten + Maus] Richtung, loslassen schlägt  [RMB] blocken  [C] Waffe  [R] wegstecken   %s" % [info.name, bar, stamina, state, aim, message]
	timer += delta
	var eye: Vector3 = player.camera.global_position
	var pose_direction := -1
	var s := 0.0
	if not is_nan(hold):
		weapon_node.global_transform = TerraforgeCombat.pose(eye, player.yaw, player.pitch, direction, hold)
		return
	match state:
		"ausholen":
			if not chosen and timer > CHOOSE_TIME:
				chosen = true
			pose_direction = direction
			s = -minf(timer / info.windup, 1.0)
			if release and timer >= info.windup:
				var id: int = terrain.attack(eye, player.yaw, player.pitch, weapon, direction)
				pending[id] = [eye, player.camera.global_basis.x, direction]
				swings += 1
				state = "schlag"
				timer = 0.0
		"schlag":
			pose_direction = direction
			s = minf(timer / _strike_time(), 1.0)
			if s >= 1.0:
				state = "erholung"
				timer = 0.0
		"erholung":
			# Von der Endlage des Schlags zurück in die Ruhe.
			weapon_node.global_transform = TerraforgeCombat.recovery_pose(eye, player.yaw, player.pitch, direction, minf(timer / info.recovery, 1.0))
			if timer >= info.recovery:
				state = "ruhe"
			return
		"block":
			pose_direction = 1
			s = -0.6
	weapon_node.global_transform = TerraforgeCombat.pose(eye, player.yaw, player.pitch, pose_direction, s)
