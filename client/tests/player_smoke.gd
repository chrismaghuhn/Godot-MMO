# Test von Bewegung und Kollision in Godot (#13), headless:
#
#   godot --headless --path client --script res://tests/player_smoke.gd
#
# Ein Körper fällt am Spawnpunkt auf die Oberfläche, läuft 4 s geradeaus (ohne einzusinken) und
# springt. Gibt eine Zeile "TERRAFORGE_PLAYER ..." aus; Exit-Code 1 bei Fehlern.
extends SceneTree

const DT := 1.0 / 60.0

var terrain: Node3D
var world: Object
var body: Object
var camera: Camera3D
var frames := 0
var start := Vector3.ZERO
var highest := -INF
var lowest_margin := INF


func _initialize() -> void:
	if not ClassDB.class_exists("TerraforgeBody"):
		fail("TerraforgeBody fehlt: Extension nicht geladen")
		return
	terrain = ClassDB.instantiate("TerraforgeTerrain")
	terrain.view_radius = 2
	terrain.far_terrain = false
	terrain.server = "off"
	root.add_child(terrain)
	camera = Camera3D.new()
	root.add_child(camera)
	camera.make_current()


func fail(message: String) -> void:
	printerr("FEHLER: " + message)
	quit(1)


func _physics_process(_delta: float) -> bool:
	frames += 1
	if frames == 1:
		world = terrain.get_world()
		if world == null:
			fail("Terrain ohne Welt")
			return true
		start = world.spawn_point()
		body = ClassDB.class_call_static("TerraforgeBody", "create", start + Vector3(0, 3, 0))
		return false
	# Erst laden lassen, dann 2 s stehen, 4 s laufen, dann springen.
	var walking := frames > 180 and frames <= 420
	var jump := frames == 450
	body.step(terrain, Vector2(0, 1) if walking else Vector2.ZERO, 0.0, jump, false, DT)
	var position: Vector3 = body.get_position()
	camera.position = position + Vector3(0, body.eye_height(), 0)
	var ground: float = world.height_at(position.x, position.z)
	if frames > 120:
		lowest_margin = minf(lowest_margin, position.y - ground)
	if frames == 180:
		if not body.is_on_ground() or absf(position.y - ground) > 0.4:
			fail("nicht gelandet: Füße %.2f, Boden %.2f" % [position.y, ground])
			return true
		start = position
	if frames == 449:
		start = position
	if frames > 450:
		highest = maxf(highest, position.y - start.y)
	if frames == 420:
		var walked := Vector2(position.x - start.x, position.z - start.z).length()
		if walked < 10.0:
			fail("nur %.1f m gelaufen" % walked)
			return true
	if frames == 540:
		if highest < 0.5 or highest > 1.2 or not body.is_on_ground():
			fail("Sprung %.2f m, am Boden %s" % [highest, body.is_on_ground()])
			return true
		if lowest_margin < -0.5:
			fail("eingesunken: %.2f m unter der Oberfläche" % -lowest_margin)
			return true
		print("TERRAFORGE_PLAYER jump=%.2f lowest=%.2f fields=%d" % [highest, lowest_margin, terrain.get_stats().field_chunks])
		quit(0)
		return true
	return false
