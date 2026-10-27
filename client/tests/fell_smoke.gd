# Test von Bäume fällen in Godot (#18), headless:
#
#   godot --headless --path client --script res://tests/fell_smoke.gd -- [off|<Server-Programm>]
#
# Stellt sich 3 m neben einen Baum außerhalb des freien Kreises am Spawn, fällt ihn (Holz kommt ins
# Inventar) und versucht es noch einmal (dann steht dort kein Baum mehr). Mit Server prüft und fällt
# der Server, mit `off` der Client selbst.
# Gibt eine Zeile "TERRAFORGE_FELL ..." aus; Exit-Code 1 bei Fehlern.
extends SceneTree

const MAX_FRAMES := 6000
const PAUSE_MS := 150

var terrain: Node3D
var camera: Camera3D
var frames := 0
var phase := "laden"
var eye := Vector3.ZERO
var tree := Vector3.ZERO
var request := -1
var waited_since := 0
var wood := 0


func _initialize() -> void:
	var args := OS.get_cmdline_user_args()
	terrain = ClassDB.instantiate("TerraforgeTerrain")
	terrain.view_radius = 2
	terrain.far_terrain = false
	var server: String = args[0] if args.size() > 0 else "off"
	terrain.server = "off" if server == "off" else "auto"
	if server != "off":
		terrain.server_binary = server
	root.add_child(terrain)
	camera = Camera3D.new()
	root.add_child(camera)
	camera.make_current()


func fail(message: String) -> void:
	printerr("FEHLER: " + message)
	quit(1)


func take_result() -> Dictionary:
	for result: Dictionary in terrain.take_edit_results():
		if result.request == request:
			return result
	return {}


func wood_count() -> int:
	var total := 0
	for stack: Dictionary in terrain.get_inventory():
		if stack.key == "wood" or stack.key == "quartz":
			total += stack.count
	return total


## Baum mit freiem Platz 3 m daneben (in +x): Auge dort auf Augenhöhe über dem Boden.
func find_tree(world: Object) -> bool:
	var spawn: Vector3 = world.spawn_point()
	var cx := floori(spawn.x / 32.0)
	var cz := floori(spawn.z / 32.0)
	for dz in range(-2, 3):
		for dx in range(-2, 3):
			for base: Vector3 in world.tree_bases(cx + dx, cz + dz):
				var stand := base + Vector3(3, 0, 0)
				var ground: float = world.height_at(stand.x, stand.z)
				if absf(ground - base.y) < 1.0 and Vector2(base.x - spawn.x, base.z - spawn.z).length() < 60.0:
					tree = base
					eye = Vector3(stand.x, ground + 1.65, stand.z)
					return true
	return false


func _process(_delta: float) -> bool:
	frames += 1
	if frames > MAX_FRAMES:
		fail("Zeit abgelaufen in Phase %s" % phase)
		return true
	if frames == 1:
		if not find_tree(terrain.get_world()):
			fail("kein Baum nahe am Spawn")
			return true
		camera.position = eye
		return false
	var stats: Dictionary = terrain.get_stats()
	match phase:
		"laden":
			if frames > 30 and stats.get("idle", false) and stats.get("field_chunks", 0) > 0:
				request = terrain.fell(eye, (tree + Vector3(0, 1.5, 0) - eye).normalized())
				phase = "fällen"
		"fällen":
			var result := take_result()
			if result.is_empty():
				return false
			if not result.ok or result.felled <= 0:
				fail("Fällen: %s" % result)
				return true
			wood = result.felled
			waited_since = Time.get_ticks_msec()
			phase = "pause"
		"pause":
			if Time.get_ticks_msec() - waited_since > PAUSE_MS and wood_count() == wood:
				request = terrain.fell(eye, (tree + Vector3(0, 1.5, 0) - eye).normalized())
				phase = "nochmal"
		"nochmal":
			var result := take_result()
			if result.is_empty():
				return false
			if result.ok or result.message != "kein Baum im Visier":
				fail("Zweites Fällen: %s" % result)
				return true
			print("TERRAFORGE_FELL server=%s wood=%d frames=%d" % [stats.get("server", "?"), wood, frames])
			quit(0)
			return true
	return false
