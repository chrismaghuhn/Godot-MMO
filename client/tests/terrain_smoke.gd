# Test des Terrain-Streamings in Godot (#11), headless:
#
#   godot --headless --path client --script res://tests/terrain_smoke.gd -- [near|far|caves] [server]
#
# near: Radius 2 ohne Fernterrain. Prüft die Spalten um den Spawnpunkt (genaue Positionen, Chunks
#       in ihrer Höhe, Bäume als MultiMesh), fliegt 10 Spalten
#       weiter und prüft, dass alte Spalten verschwinden.
# far:  Radius 4 mit Fernterrain. Prüft, dass genau die Spalten und Fernkacheln als Nodes da sind,
#       auch nach einem Flug über ein ganzes Fernfeld (1024 m).
# caves: wie near, aber mit Server (#56; Pfad des Programms als zweites Argument): Verbindung steht
#       und Höhlen-Chunks kommen an.
# near und far laufen ohne Server (nur öffentliche Oberfläche).
# Gibt eine Zeile "TERRAFORGE_TERRAIN ..." aus; Exit-Code 1 bei Fehlern.
extends SceneTree

const MAX_FRAMES := 5000
## 3×3 Stufe-2-Felder mit je 4×4 Stufe-1-Kacheln, dazu 40 Stufe-2-Kacheln außen.
const FAR_TILES := 9 * 16 + 40

var far := false
var caves := false
var radius := 2
var terrain: Node3D
var camera: Camera3D
var frames := 0
var quiet := 0
var phase := -1
var first := {}


func _initialize() -> void:
	var args := OS.get_cmdline_user_args()
	far = args.has("far")
	caves = args.has("caves")
	radius = 4 if far else 2
	if not ClassDB.class_exists("TerraforgeTerrain"):
		fail("TerraforgeTerrain fehlt: Extension nicht geladen")
		return
	terrain = ClassDB.instantiate("TerraforgeTerrain")
	terrain.view_radius = radius
	terrain.far_terrain = far
	terrain.server = "auto" if caves else "off"
	if caves:
		terrain.server_binary = args[args.find("caves") + 1] if args.size() > args.find("caves") + 1 else ""
	root.add_child(terrain)
	camera = Camera3D.new()
	root.add_child(camera)
	camera.make_current()


func fail(message: String) -> void:
	printerr("FEHLER: " + message)
	quit(1)


## Spalten-Koordinaten (x, z) um die Kamera.
func expected_columns() -> Dictionary:
	var center := Vector2i(floori(camera.position.x / 32.0), floori(camera.position.z / 32.0))
	var wanted := {}
	for dz in range(-radius, radius + 1):
		for dx in range(-radius, radius + 1):
			wanted[center + Vector2i(dx, dz)] = true
	return wanted


func check(stats: Dictionary) -> bool:
	var wanted := expected_columns()
	var columns := {}
	var other_nodes := 0
	var trees := 0
	for node in terrain.get_children():
		if not String(node.name).begins_with("Spalte"):
			other_nodes += 1
			var mesh: ArrayMesh = node.mesh
			if mesh == null or mesh.get_surface_count() != 1:
				fail("leeres Mesh in %s" % node.name)
				return false
			continue
		var key := Vector2i(roundi(node.position.x / 32.0), roundi(node.position.z / 32.0))
		if not wanted.has(key) or columns.has(key) or not is_equal_approx(node.position.y, 0.0):
			fail("Spalte an falscher Stelle: %s bei %s" % [node.name, node.position])
			return false
		columns[key] = true
		for chunk in node.get_children():
			if chunk is MultiMeshInstance3D:
				trees += chunk.multimesh.instance_count
				# Headless speichert Godot keine Instanz-Transforms (Dummy-Renderer); die prüft
				# ein Rust-Test (`gdext/src/trees.rs`), den freien Kreis ein Core-Test.
				var multimesh: MultiMesh = chunk.multimesh
				if multimesh.instance_count == 0 or multimesh.mesh == null:
					fail("leeres MultiMesh in %s" % node.name)
					return false
				# Diese Werte liegen am Objekt und lassen sich auch headless prüfen.
				if multimesh.transform_format != MultiMesh.TRANSFORM_3D or chunk.material_override == null:
					fail("MultiMesh in %s ohne 3D-Transforms oder Material" % node.name)
					return false
				continue
			var mesh: ArrayMesh = chunk.mesh
			if mesh == null or mesh.get_surface_count() != 1 or mesh.surface_get_array_len(0) == 0:
				fail("leeres Mesh in %s" % node.name)
				return false
			if chunk.position.x != 0.0 or chunk.position.z != 0.0 or fmod(chunk.position.y, 32.0) != 0.0:
				fail("Chunk an falscher Stelle in %s: %s" % [node.name, chunk.position])
				return false
	if columns.size() != wanted.size() or stats.columns != wanted.size():
		fail("%d Spalten-Nodes, %d laut Streamer, erwartet %d" % [columns.size(), stats.columns, wanted.size()])
		return false
	if trees != stats.trees or trees == 0:
		fail("%d Bäume in MultiMeshes, %d laut Terrain" % [trees, stats.trees])
		return false
	var expected_far: int = FAR_TILES if far else 0
	if other_nodes != stats.far_tiles + stats.cave_meshes or stats.far_tiles != expected_far:
		fail("%d Fern- und Höhlen-Nodes, %d + %d laut Streamer, erwartet %d Fernkacheln" % [other_nodes, stats.far_tiles, stats.cave_meshes, expected_far])
		return false
	if caves and (stats.server != "verbunden" or stats.cave_meshes == 0):
		fail("Server %s, %d Höhlen-Chunks" % [stats.server, stats.cave_meshes])
		return false
	if not caves and (stats.server != "aus" or stats.cave_meshes != 0):
		fail("ohne Server: Server %s, %d Höhlen-Chunks" % [stats.server, stats.cave_meshes])
		return false
	return true


func _process(_delta: float) -> bool:
	frames += 1
	if frames > MAX_FRAMES:
		fail("Terrain kommt nicht zur Ruhe")
		return true
	if phase == -1:
		# Erst hier: `_ready` des Terrains (und damit die Welt) gibt es nach dem ersten Frame.
		var world: Object = terrain.get_world()
		if world == null:
			fail("Terrain ohne Welt")
			return true
		camera.position = world.spawn_point() + Vector3(0, 4.5, 0)
		phase = 0
		return false
	var stats: Dictionary = terrain.get_stats()
	quiet = quiet + 1 if stats.idle else 0
	if quiet < 10:
		return false
	quiet = 0
	if not check(stats):
		return true
	if phase == 0:
		first = stats
		# Weiter fliegen: alles Alte muss weg (bzw. ersetzt sein).
		camera.position.x += 1024.0 if far else 10 * 32.0
		phase = 1
		return false
	var mode := "far" if far else ("caves" if caves else "near")
	print("TERRAFORGE_TERRAIN mode=%s columns=%d meshes=%d trees=%d far_tiles=%d caves=%d frames=%d" % [mode, first.columns, first.meshes, first.trees, stats.far_tiles, first.cave_meshes, frames])
	quit(0)
	return true
