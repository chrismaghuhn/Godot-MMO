# Trainingspuppen (#62): Meshes aus dem Core, an den Stellen, die der Server meldet. Bei einem
# Treffer kippt die Puppe nach Wucht und Richtung und schwingt zurück (gedämpfte Feder); der Schaden
# steigt als Zahl über ihr auf.
extends Node3D

const SPRING := 60.0
const DAMPING := 7.0
const LABEL_TIME := 1.2

var terrain: TerraforgeTerrain
var nodes: Array[Node3D] = []
## Je Puppe: Neigung (Achse * Winkel) und Winkelgeschwindigkeit.
var tilt: Array[Vector3] = []
var spin: Array[Vector3] = []
var labels: Array = []
var hits := 0


func _process(delta: float) -> void:
	if nodes.is_empty():
		_create()
	for i in nodes.size():
		spin[i] += (-tilt[i] * SPRING - spin[i] * DAMPING) * delta
		tilt[i] += spin[i] * delta
		var angle := tilt[i].length()
		var basis := Basis(Vector3.UP, nodes[i].get_meta("yaw"))
		if angle > 1e-4:
			basis = Basis(tilt[i] / angle, angle) * basis
		nodes[i].basis = basis
	for entry in labels.duplicate():
		var label: Label3D = entry[0]
		entry[1] += delta
		label.position.y += delta * 0.6
		label.modulate.a = 1.0 - entry[1] / LABEL_TIME
		if entry[1] > LABEL_TIME:
			label.queue_free()
			labels.erase(entry)


func _create() -> void:
	var material := StandardMaterial3D.new()
	material.vertex_color_use_as_albedo = true
	material.vertex_color_is_srgb = true
	for dummy: Dictionary in terrain.get_dummies():
		var node := MeshInstance3D.new()
		node.mesh = TerraforgeCombat.dummy_mesh()
		node.material_override = material
		node.position = dummy.feet
		node.set_meta("yaw", dummy.yaw)
		add_child(node)
		nodes.append(node)
		tilt.append(Vector3.ZERO)
		spin.append(Vector3.ZERO)


## Treffer vom Server: kippt die Puppe in Schlagrichtung (seitlich von links/rechts, sonst vom
## Spieler weg). `eye` und `right` vom Spieler, `direction` wie bei `TerraforgeCombat`.
func hit(info: Dictionary, eye: Vector3, right: Vector3, direction: int) -> void:
	var index: int = info.dummy
	if index >= nodes.size():
		return
	hits += 1
	var away := nodes[index].global_position - eye
	away.y = 0.0
	var push: Vector3 = right if direction == 1 else (-right if direction == 2 else away.normalized())
	# Drehung um oben × Schubrichtung kippt die Spitze in Schubrichtung.
	spin[index] += Vector3.UP.cross(push).normalized() * minf(info.impulse * 0.12, 4.0)
	var label := Label3D.new()
	label.text = "%.0f %s" % [info.damage, info.zone_name]
	label.billboard = BaseMaterial3D.BILLBOARD_ENABLED
	label.font_size = 48
	label.position = nodes[index].position + Vector3(0, 2.0, 0)
	label.modulate = Color(1.0, 0.85, 0.3) if info.zone_name == "Kopf" else Color(1, 1, 1)
	add_child(label)
	labels.append([label, 0.0])
