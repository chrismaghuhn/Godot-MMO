# Test des Spielstands in Godot (#19), headless, mit Server:
#
#   godot --headless --path client --script res://tests/persist_smoke.gd -- <Server-Programm> <Ordner> first|second <Abstand>|broken
#
# first:  gräbt am Spawnpunkt eine Grube, baut schräg nach vorn aus dem Inventar und beendet sich
#         (der Server speichert beim Ende). Gibt den freien Abstand in Blickrichtung aus.
# second: startet neu mit demselben Ordner; die Grube ist da (auch für die Kollision), das Gebaute
#         steht wieder im selben Abstand (dafür braucht der Client die veränderten Chunks vom
#         Server), und die Ausbeute liegt im Inventar.
# broken: der Spielstand ist kaputt; der Server startet nicht, Graben ist gesperrt.
# Gibt eine Zeile "TERRAFORGE_PERSIST ..." aus; Exit-Code 1 bei Fehlern.
extends SceneTree

const MAX_FRAMES := 6000

var terrain: Node3D
var camera: Camera3D
var frames := 0
var phase := "laden"
var mode := ""
var eye := Vector3.ZERO
var pit := Vector3.ZERO
var request := -1
var forward := Vector3(0, -0.3, -1).normalized()
var expected := -1.0
var waited_since := 0


func _initialize() -> void:
	var args := OS.get_cmdline_user_args()
	if args.size() < 3:
		fail("Argumente: Server-Programm, Ordner, first|second")
		return
	mode = args[2]
	if args.size() > 3:
		expected = float(args[3])
	terrain = ClassDB.instantiate("TerraforgeTerrain")
	terrain.view_radius = 2
	terrain.far_terrain = false
	terrain.server = "auto"
	terrain.server_binary = args[0]
	terrain.save_dir = args[1]
	root.add_child(terrain)
	camera = Camera3D.new()
	root.add_child(camera)
	camera.make_current()


func fail(message: String) -> void:
	printerr("FEHLER: " + message)
	quit(1)


func idle() -> bool:
	var stats: Dictionary = terrain.get_stats()
	return stats.get("idle", false) and stats.get("field_chunks", 0) > 0


## Abstand bis zum ersten festen Punkt in Blickrichtung `forward` (höchstens 8 m).
func free_distance() -> float:
	var distance := 0.0
	while distance < 8.0 and terrain.density_at(eye + forward * distance) <= 0.0:
		distance += 0.05
	return distance


func inventory_total() -> int:
	var total := 0
	for stack: Dictionary in terrain.get_inventory():
		total += stack.count
	return total


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
	if mode == "broken":
		if terrain.get_stats().get("server", "") != "Spielstand defekt":
			fail("Server-Zustand %s statt „Spielstand defekt“" % terrain.get_stats().get("server", "?"))
			return true
		if request < 0:
			request = terrain.dig(eye, Vector3.DOWN, 3)
			return false
		for result: Dictionary in terrain.take_edit_results():
			if result.request == request:
				if result.ok or not result.message.begins_with("Spielstand nicht geladen"):
					fail("Graben trotz defektem Spielstand: %s" % result)
					return true
				print("TERRAFORGE_PERSIST broken")
				quit(0)
				return true
		return false
	if terrain.get_stats().get("server", "") != "verbunden":
		fail("kein Server: %s" % terrain.get_stats().get("server", "?"))
		return true
	match phase:
		"laden":
			if frames > 30 and idle():
				if mode == "first":
					if terrain.density_at(pit) <= 0.0 or inventory_total() != 0:
						fail("Spielstand nicht leer: Grube schon da oder Inventar %d" % inventory_total())
						return true
					request = terrain.dig(eye, Vector3.DOWN, 3)
					phase = "graben"
				else:
					phase = "prüfen"
		"graben":
			for result: Dictionary in terrain.take_edit_results():
				if result.request == request:
					if not result.ok:
						fail("Graben: %s" % result)
						return true
					waited_since = Time.get_ticks_msec()
					phase = "bauen"
		"bauen":
			if Time.get_ticks_msec() - waited_since > 150 and idle():
				var slot: int = terrain.get_inventory()[0].slot
				request = terrain.build(eye, forward, slot)
				phase = "gebaut"
		"gebaut":
			for result: Dictionary in terrain.take_edit_results():
				if result.request == request:
					if not result.ok or result.placed == 0:
						fail("Bauen: %s" % result)
						return true
					phase = "gebaut laden"
		"gebaut laden":
			if idle() and free_distance() < 7.5:
				print("TERRAFORGE_PERSIST first free=%.2f inventory=%d" % [free_distance(), inventory_total()])
				quit(0)
				return true
		"prüfen":
			# Die veränderten Chunks kommen vom Server, ohne eigene Aktion in diesem Lauf.
			if terrain.get_stats().get("touched_chunks", 0) == 0:
				if frames > 900:
					fail("keine veränderten Chunks vom Server")
					return true
				return false
			if idle() and terrain.density_at(pit) <= 0.0 and inventory_total() > 0 and absf(free_distance() - expected) < 0.01:
				print("TERRAFORGE_PERSIST second free=%.2f inventory=%d" % [free_distance(), inventory_total()])
				quit(0)
				return true
			if frames > 900:
				fail("nach dem Neustart: Dichte %.2f an der Grube, Inventar %d, freier Abstand %.2f statt %.2f" % [terrain.density_at(pit), inventory_total(), free_distance(), expected])
				return true
	return false
