# Test von Graben und Bauen in Godot (#14), headless:
#
#   godot --headless --path client --script res://tests/dig_smoke.gd -- [off|<Server-Programm>]
#
# Am Spawnpunkt: Graben nach unten macht den Boden unter den Füßen zu Luft (auch im Dichtefeld der
# Kollision) und legt die Ausbeute ins Inventar (#18). Bauen schräg nach vorn setzt Material vom
# ersten Inventarplatz näher ans Auge und verbraucht es; Bauen in den eigenen Körper oder von einem
# leeren Platz wird abgelehnt. Mit Server prüft und gräbt der Server, mit `off` der Client selbst.
# Gibt eine Zeile "TERRAFORGE_DIG ..." aus; Exit-Code 1 bei Fehlern.
extends SceneTree

const MAX_FRAMES := 6000
## Das Mindest-Intervall des Servers zwischen zwei Aktionen ist 100 ms.
const PAUSE_MS := 150

var terrain: Node3D
var camera: Camera3D
var frames := 0
var phase := "laden"
var eye := Vector3.ZERO
var pit := Vector3.ZERO
var forward := Vector3(0, -0.3, -1).normalized()
var free_before := 0.0
var request := -1
var waited_since := 0
var summary := ""
var count_before := 0
var placed := 0


func _initialize() -> void:
	var args := OS.get_cmdline_user_args()
	if not ClassDB.class_exists("TerraforgeTerrain"):
		fail("TerraforgeTerrain fehlt: Extension nicht geladen")
		return
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


## Abstand bis zum ersten festen Punkt in Blickrichtung (höchstens 8 m).
func free_distance(direction: Vector3) -> float:
	var distance := 0.0
	while distance < 8.0 and terrain.density_at(eye + direction * distance) <= 0.0:
		distance += 0.25
	return distance


## Das Ergebnis der laufenden Anfrage, sobald es da ist, sonst ein leeres Dictionary.
func take_result() -> Dictionary:
	for result: Dictionary in terrain.take_edit_results():
		if result.request == request:
			return result
	return {}


## Alle Stück im Inventar.
func inventory_total() -> int:
	var total := 0
	for stack: Dictionary in terrain.get_inventory():
		total += stack.count
	return total


func idle() -> bool:
	var stats: Dictionary = terrain.get_stats()
	return stats.get("idle", false) and stats.get("field_chunks", 0) > 0


func _process(_delta: float) -> bool:
	frames += 1
	if frames > MAX_FRAMES:
		fail("Zeit abgelaufen in Phase %s" % phase)
		return true
	if frames == 1:
		var spawn: Vector3 = terrain.get_world().spawn_point()
		eye = spawn + Vector3(0, 1.65, 0)
		pit = spawn - Vector3(0, 0.6, 0)
		camera.position = eye
		return false
	match phase:
		"laden":
			if frames > 30 and idle():
				if terrain.density_at(pit) <= 0.0:
					fail("kein Boden unter dem Spawnpunkt")
					return true
				request = terrain.dig(eye, Vector3.DOWN, 3)
				phase = "graben"
		"graben":
			var result := take_result()
			if result.is_empty():
				return false
			if not result.ok or result.removed.is_empty():
				fail("Graben: %s" % result)
				return true
			summary = result.message
			phase = "neu laden"
		"neu laden":
			# Der alte Stand gilt, bis der neue da ist; dann ist die Grube auch für die Kollision Luft.
			if idle() and terrain.density_at(pit) <= 0.0 and inventory_total() > 0:
				var first: Dictionary = terrain.get_inventory()[0]
				if first.hardness <= 0.0 or first.density <= 0.0 or first.details.is_empty():
					fail("Item ohne Werte: %s" % first)
					return true
				request = terrain.build(eye, forward, 30)
				phase = "leerer Platz"
		"leerer Platz":
			var result := take_result()
			if result.is_empty():
				return false
			if result.ok or result.message != "nichts zum Bauen auf diesem Platz":
				fail("Bauen von leerem Platz: %s" % result)
				return true
			waited_since = Time.get_ticks_msec()
			phase = "pause"
		"pause":
			if Time.get_ticks_msec() - waited_since > PAUSE_MS:
				request = terrain.build(eye, Vector3.DOWN, 0)
				phase = "in den Körper"
		"in den Körper":
			var result := take_result()
			if result.is_empty():
				return false
			if result.ok or result.message != "zu nah am Spieler":
				fail("Bauen in den Körper: %s" % result)
				return true
			waited_since = Time.get_ticks_msec()
			phase = "pause bauen"
		"pause bauen":
			if Time.get_ticks_msec() - waited_since > PAUSE_MS:
				free_before = free_distance(forward)
				count_before = inventory_total()
				request = terrain.build(eye, forward, 0)
				phase = "bauen"
		"bauen":
			var result := take_result()
			if result.is_empty():
				return false
			if not result.ok or result.placed == 0:
				fail("Bauen: %s" % result)
				return true
			placed = result.placed
			phase = "gebaut laden"
		"gebaut laden":
			if idle() and free_distance(forward) < free_before - 0.5 and inventory_total() == count_before - placed:
				print("TERRAFORGE_DIG server=%s dug=\"%s\" free=%.2f->%.2f inventory=%d->%d frames=%d" % [terrain.get_stats().server, summary, free_before, free_distance(forward), count_before, inventory_total(), frames])
				quit(0)
				return true
	return false
