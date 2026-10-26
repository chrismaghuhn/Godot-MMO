# Smoke-Test der Rust-Anbindung (#10): lädt die Extension und fragt Höhe und Biom ab.
#
#   godot --headless --path client --script res://tests/gdext_smoke.gd -- <seed> <x> <z>
#
# Gibt eine Zeile "TERRAFORGE height=<h> biome=<name> sea=<s> spawn=<x>,<y>,<z>" aus. Exit-Code 1, wenn die
# Extension fehlt oder falsch antwortet, 2 bei ungültigen Argumenten.
# `gdext/tests/godot_smoke.rs` vergleicht die Werte mit terraforge-core.
extends SceneTree


func _init() -> void:
	var args := OS.get_cmdline_user_args()
	if args.size() != 3 or not args[0].is_valid_int() or not args[1].is_valid_float() or not args[2].is_valid_float():
		printerr("Aufruf: ... --script res://tests/gdext_smoke.gd -- <seed> <x> <z>")
		quit(2)
		return
	var world_seed := int(args[0])
	var x := float(args[1])
	var z := float(args[2])

	if not ClassDB.class_exists("TerraforgeWorld"):
		printerr("TerraforgeWorld fehlt: Extension nicht geladen (siehe client/README.md)")
		quit(1)
		return

	# Über ClassDB statt direkt per Klassenname: Fehlt die Extension, wäre das sonst ein Parse-Fehler.
	var world: Object = ClassDB.class_call_static("TerraforgeWorld", "create", world_seed)
	if world == null or world.seed() != world_seed:
		printerr("Welt für Seed %d nicht erzeugt" % world_seed)
		quit(1)
		return
	# Seeds außerhalb von u32 ergeben keine Welt (sonst still eine andere als beim Server).
	if ClassDB.class_call_static("TerraforgeWorld", "create", -1) != null:
		printerr("Seed -1 wurde angenommen")
		quit(1)
		return
	var spawn: Vector3 = world.spawn_point()
	print("TERRAFORGE height=%.6f biome=%s sea=%.6f spawn=%.3f,%.3f,%.3f" % [world.height_at(x, z), world.biome_at(x, z), world.sea_level(), spawn.x, spawn.y, spawn.z])
	quit(0)
