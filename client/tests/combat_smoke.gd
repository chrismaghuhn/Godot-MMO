# Test des Kampfs in Godot (#62), headless:
#
#   godot --headless --path client --script res://tests/combat_smoke.gd -- [off|<Server-Programm>]
#
# Der Spieler steht vor der Trainingspuppe, zieht die Waffe (R), holt mit der Maus aus und lässt
# los, dreimal im Takt des Clients: Axt von oben, dann Wechsel aufs Schwert (leichter, kürzer
# ausgeholt) von oben, dann ein Stich. Jeder Schlag trifft, keiner kommt beim Server „zu früh“ an
# (Tempo von Client und Server passt zusammen, auch nach dem Wechsel), und die Puppe kippt.
# Gibt eine Zeile "TERRAFORGE_COMBAT ..." aus; Exit-Code 1 bei Fehlern.
extends SceneTree

const MAX_FRAMES := 3000
const SWINGS := 3

var terrain: TerraforgeTerrain
var player: Node3D
var fighter: Node
var dummies: Node3D
var frames := 0
var phase := "laden"
var messages: Array[String] = []


func _initialize() -> void:
	var args := OS.get_cmdline_user_args()
	terrain = TerraforgeTerrain.new()
	terrain.view_radius = 2
	terrain.far_terrain = false
	var server: String = args[0] if args.size() > 0 else "off"
	terrain.server = "off" if server == "off" else "auto"
	if server != "off":
		terrain.server_binary = server
	root.add_child(terrain)


func fail(message: String) -> void:
	printerr("FEHLER: " + message)
	quit(1)


func _process(_delta: float) -> bool:
	frames += 1
	if frames > MAX_FRAMES:
		fail("Zeit abgelaufen in Phase %s (%s)" % [phase, messages])
		return true
	match phase:
		"laden":
			var list: Array = terrain.get_dummies()
			if list.is_empty():
				return false
			var dummy: Dictionary = list[0]
			var feet: Vector3 = dummy.feet
			# Die Puppe schaut in Richtung (−sin yaw, −cos yaw); dort 1,3 m vor ihr stehen.
			var facing := Vector3(-sin(dummy.yaw), 0, -cos(dummy.yaw))
			var stand := feet + facing * 1.3
			stand.y = terrain.get_world().height_at(stand.x, stand.z)
			player = preload("res://scripts/player.gd").new()
			root.add_child(player)
			player.setup(terrain, stand, feet + Vector3(0, 1.5, 0))
			player.camera.make_current()
			dummies = preload("res://scripts/dummies.gd").new()
			dummies.terrain = terrain
			root.add_child(dummies)
			fighter = preload("res://scripts/fighter.gd").new()
			fighter.terrain = terrain
			fighter.player = player
			fighter.dummies = dummies
			root.add_child(fighter)
			phase = "warten"
		"warten":
			if frames > 60 and terrain.get_stats().get("idle", false) and player.body.is_on_ground():
				fighter._set_weapon(1)
				fighter.set_active(true)
				phase = "schlagen"
		"schlagen":
			if fighter.state == "ruhe" and fighter.swings < SWINGS:
				if fighter.swings == 1:
					fighter._set_weapon(0)
				fighter.start_windup()
				var motion := InputEventMouseMotion.new()
				# Dritter Schlag: Maus nach unten = Stich.
				motion.relative = Vector2(0, 20 if fighter.swings == 2 else -20)
				fighter._unhandled_input(motion)
				fighter.release = true
			if fighter.message != "" and (messages.is_empty() or messages[-1] != "%d:%s" % [fighter.swings, fighter.message]):
				messages.append("%d:%s" % [fighter.swings, fighter.message])
			if fighter.swings == SWINGS and fighter.state == "ruhe" and dummies.hits == SWINGS:
				for entry in messages:
					if "zu früh" in entry or "daneben" in entry or "erschöpft" in entry:
						fail("Schlag nicht angekommen: %s" % [messages])
						return true
				var tilted: Vector3 = dummies.tilt[0]
				print("TERRAFORGE_COMBAT server=%s hits=%d stamina=%.0f last=\"%s\" frames=%d" % [terrain.get_stats().get("server", "?"), dummies.hits, fighter.stamina, fighter.message, frames])
				quit(0)
				return true
	return false
