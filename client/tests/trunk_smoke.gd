# Test der Kollision mit Baumstämmen in Godot (#58), headless, ohne Server:
#
#   godot --headless --path client --script res://tests/trunk_smoke.gd
#
# Ein Körper steht 4 m vor einem Baum und läuft 3 s darauf zu: Er bleibt vor dem Stamm stehen.
# Dann wird der Baum gefällt, und derselbe Weg führt durch die Stelle hindurch.
# Gibt eine Zeile "TERRAFORGE_TRUNK ..." aus; Exit-Code 1 bei Fehlern.
extends SceneTree

const DT := 1.0 / 60.0
const MAX_FRAMES := 4000

var terrain: Node3D
var world: Object
var body: Object
var camera: Camera3D
var frames := 0
var phase := "laden"
var tree := Vector3.ZERO
var start := Vector3.ZERO
var yaw := 0.0
var walked := 0
var closest := INF
var request := -1
## Lage nach 2 s Laufen: Danach darf sich der Körper vor dem Stamm nicht mehr bewegen.
var settled := Vector3.ZERO


func _initialize() -> void:
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


## Ein Baum nahe am Spawn mit ebenem Weg 4 m in +x davor.
func find_tree() -> bool:
	var spawn: Vector3 = world.spawn_point()
	var cx := floori(spawn.x / 32.0)
	var cz := floori(spawn.z / 32.0)
	for dz in range(-2, 3):
		for dx in range(-2, 3):
			for base: Vector3 in world.tree_bases(cx + dx, cz + dz):
				var flat := true
				for step in range(0, 9):
					var x := base.x + step * 0.5
					flat = flat and absf(world.height_at(x, base.z) - base.y) < 0.6
				if flat:
					tree = base
					start = Vector3(base.x + 4.0, world.height_at(base.x + 4.0, base.z) + 0.5, base.z)
					return true
	return false


func _physics_process(_delta: float) -> bool:
	frames += 1
	if frames > MAX_FRAMES:
		fail("Zeit abgelaufen in Phase %s" % phase)
		return true
	if frames == 1:
		world = terrain.get_world()
		if not find_tree():
			fail("kein Baum mit freiem Weg nahe am Spawn")
			return true
		camera.position = start + Vector3(0, 1.6, 0)
		body = ClassDB.class_call_static("TerraforgeBody", "create", start)
		# Blick nach −x (zum Baum): yaw = 90°.
		yaw = PI / 2
		return false
	var stats: Dictionary = terrain.get_stats()
	match phase:
		"laden":
			body.step(terrain, Vector2.ZERO, yaw, false, false, DT)
			if frames > 60 and stats.get("idle", false) and body.is_on_ground():
				phase = "hin"
		"hin", "durch":
			body.step(terrain, Vector2(0, 1), yaw, false, false, DT)
			var position: Vector3 = body.get_position()
			camera.position = position + Vector3(0, 1.6, 0)
			closest = minf(closest, Vector2(position.x - tree.x, position.z - tree.z).length())
			walked += 1
			if walked == 120:
				settled = position
			if walked < 180:
				return false
			if phase == "hin":
				if closest < 0.45 or position.x < tree.x:
					fail("durch den Stamm gelaufen: nächster Abstand %.2f, Körper bei %s, Baum bei %s" % [closest, position, tree])
					return true
				# Stehen geblieben (der Abstand hängt von Art und Größe des Baums ab, nicht prüfen).
				if position.distance_to(settled) > 0.1 or closest > 3.0:
					fail("vor dem Stamm nicht stehen geblieben: %s → %s, nächster Abstand %.2f" % [settled, position, closest])
					return true
				print("TERRAFORGE_TRUNK stop=%.2f" % closest)
				request = terrain.fell(position + Vector3(0, 1.65, 0), (tree + Vector3(0, 1.5, 0) - position - Vector3(0, 1.65, 0)).normalized())
				phase = "fällen"
			else:
				if position.x > tree.x - 1.0:
					fail("nach dem Fällen nicht durchgekommen: Körper bei %s, Baum bei %s" % [position, tree])
					return true
				print("TERRAFORGE_TRUNK through=%.2f" % (tree.x - position.x))
				quit(0)
				return true
		"fällen":
			body.step(terrain, Vector2.ZERO, yaw, false, false, DT)
			for result: Dictionary in terrain.take_edit_results():
				if result.request == request:
					if not result.ok:
						fail("Fällen: %s" % result)
						return true
					walked = 0
					phase = "durch"
	return false
