# Startszene (#11): Terrain-Streaming, Spieler (#13), Licht, Nebel, Wasser und Overlay.
#
# Aufbau im Code statt im Editor, damit Werte aus dem Core (Spawnpunkt, Sichtweite) passen.
# Umgebungsvariablen, soweit möglich wie im Viewer: TERRAFORGE_VIEW_RADIUS, TERRAFORGE_FAR
# (0 = ohne Fernterrain), TERRAFORGE_AUTOPILOT (m/s nach +x), TERRAFORGE_BENCH (Sekunden, > 0),
# TERRAFORGE_BENCH_OUT, TERRAFORGE_SHOT_AFTER (Frames, Standard 240), TERRAFORGE_PITCH/
# TERRAFORGE_YAW (Grad), TERRAFORGE_CAM_Y (fliegend so viel höher starten), TERRAFORGE_FLY (fliegend
# starten; Autopilot fliegt immer), TERRAFORGE_SERVER (auto, off oder
# host:port; Standard auto, siehe TerraforgeTerrain), TERRAFORGE_FIGHT (Kampf-Schnappschuss: vor die
# Puppe stellen, Waffe ziehen und in Richtung 0-3 ausholen, ohne loszulassen; mit
# TERRAFORGE_WEAPON 0-2 und TERRAFORGE_FIGHT_S, Lage im Schlag 0-1, negativ beim Ausholen). Anders als im Viewer: TERRAFORGE_SEED
# (im Viewer das erste Argument) und TERRAFORGE_SHOT ist der Pfad der PNG-Datei (im Viewer ein
# Schalter für shots/auto.png).
extends Node3D

const SKY_COLOR := Color(0.58, 0.72, 0.86)
const FOG_COLOR := Color(0.62, 0.74, 0.86)
const AMBIENT_COLOR := Color(0.70, 0.82, 0.96)
## Wasser: feines Feld um die Kamera (2 m Raster, für die Wellen im Shader) und ein flacher Ring
## außen herum bis zum Horizont. Ein durchgehend feines Feld hätte über 1 Mio. Vertices.
const WATER_INNER := 1024.0
const WATER_INNER_SUBDIVISIONS := 511
const WATER_OUTER := 8192.0
## Höchstens so viele Versuche, wenn das Fenster für den Screenshot noch keine Größe hat.
const SHOT_RETRIES := 20

var terrain: TerraforgeTerrain
var world: TerraforgeWorld
var player: Node3D
## Wasserflächen (inneres Feld und Ring), gemeinsam verschoben.
var water: Node3D
var autopilot := 0.0


static func env_float(name: String, fallback: float) -> float:
	var value := OS.get_environment(name)
	return float(value) if value.is_valid_float() else fallback


func _ready() -> void:
	terrain = TerraforgeTerrain.new()
	terrain.seed = int(env_float("TERRAFORGE_SEED", 42))
	# Wie im Viewer begrenzt (darüber ragt die Nahzone aus der feinen Fernstufe heraus).
	terrain.view_radius = clampi(int(env_float("TERRAFORGE_VIEW_RADIUS", 8)), 1, 30)
	terrain.far_terrain = env_float("TERRAFORGE_FAR", 1) != 0
	if OS.has_environment("TERRAFORGE_SERVER"):
		terrain.server = OS.get_environment("TERRAFORGE_SERVER")
	var bench_seconds := env_float("TERRAFORGE_BENCH", 0.0)
	terrain.record_job_times = bench_seconds > 0.0
	add_child(terrain)
	world = terrain.get_world()
	if world == null:
		push_error("Terrain ohne Welt (Seed ungültig?)")
		return

	var spawn := world.spawn_point()
	player = preload("res://scripts/player.gd").new()
	add_child(player)
	player.setup(terrain, spawn, world.spawn_look(spawn))
	player.camera.far = TerraforgeWorld.far_view() * 2.0
	player.camera.make_current()
	if OS.has_environment("TERRAFORGE_PITCH"):
		player.look(deg_to_rad(env_float("TERRAFORGE_YAW", 0)), deg_to_rad(env_float("TERRAFORGE_PITCH", 0)))
	autopilot = env_float("TERRAFORGE_AUTOPILOT", 0.0)
	# Wie bisher im Viewer: Augenhöhe 4,5 m über dem Spawn, wenn fliegend gestartet wird.
	if autopilot != 0.0 or OS.has_environment("TERRAFORGE_FLY") or OS.has_environment("TERRAFORGE_CAM_Y"):
		player.set_flying(true)
		player.position = spawn + Vector3(0, 4.5 - player.body.eye_height() + env_float("TERRAFORGE_CAM_Y", 0.0), 0)
		player.reset_physics_interpolation()

	_add_environment()
	_add_light()
	_add_water()
	var overlay := preload("res://scripts/overlay.gd").new()
	overlay.terrain = terrain
	overlay.world = world
	overlay.player = player
	add_child(overlay)
	var dummies := preload("res://scripts/dummies.gd").new()
	dummies.terrain = terrain
	add_child(dummies)
	var fighter := preload("res://scripts/fighter.gd").new()
	fighter.name = "Fighter"
	fighter.terrain = terrain
	fighter.player = player
	fighter.dummies = dummies
	add_child(fighter)
	if OS.has_environment("TERRAFORGE_FIGHT"):
		_fight_pose.call_deferred(fighter, dummies)
	var inventory := preload("res://scripts/inventory.gd").new()
	inventory.terrain = terrain
	inventory.player = player
	add_child(inventory)
	if bench_seconds > 0.0:
		var bench := preload("res://scripts/bench.gd").new()
		bench.terrain = terrain
		bench.seconds = bench_seconds
		add_child(bench)
	if OS.has_environment("TERRAFORGE_SHOT"):
		_take_screenshot_later(OS.get_environment("TERRAFORGE_SHOT"), int(env_float("TERRAFORGE_SHOT_AFTER", 240)))


## Für Schnappschüsse: vor die Puppe stellen und mit gezogener Waffe ausholen (nicht loslassen).
func _fight_pose(fighter: Node, dummies: Node3D) -> void:
	var list: Array = terrain.get_dummies()
	if list.is_empty():
		_fight_pose.call_deferred(fighter, dummies)
		return
	var dummy: Dictionary = list[0]
	var feet: Vector3 = dummy.feet
	var stand: Vector3 = feet + Vector3(-sin(dummy.yaw), 0, -cos(dummy.yaw)) * 1.4
	stand.y = world.height_at(stand.x, stand.z)
	player.body.set_position(stand)
	player.position = stand
	player.reset_physics_interpolation()
	var to := feet + Vector3(0, 1.3, 0) - (stand + Vector3(0, 1.65, 0))
	player.look(atan2(-to.x, -to.z), atan2(to.y, Vector2(to.x, to.z).length()))
	fighter._set_weapon(int(env_float("TERRAFORGE_WEAPON", 0)))
	fighter.set_active(true)
	fighter.start_windup()
	fighter.direction = int(env_float("TERRAFORGE_FIGHT", 0))
	fighter.chosen = true
	fighter.hold = env_float("TERRAFORGE_FIGHT_S", -1.0)


func _add_environment() -> void:
	var env := Environment.new()
	env.background_mode = Environment.BG_COLOR
	env.background_color = SKY_COLOR
	env.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	env.ambient_light_color = AMBIENT_COLOR
	env.ambient_light_energy = 0.35
	env.tonemap_mode = Environment.TONE_MAPPER_FILMIC
	# Mit Fernterrain reicht die Sicht bis ~3,5 km. Der Nebel endet vor der garantierten Reichweite
	# (Kamera am Feldrand), sonst sieht man die Kante; ohne Fernterrain am Rand der Nahzone.
	var fog_end := TerraforgeWorld.far_guaranteed() * 0.95
	var fog_start := fog_end * 0.30
	if not terrain.far_terrain:
		fog_end = terrain.view_radius * TerraforgeWorld.chunk_size() - 20.0
		fog_start = fog_end * 0.55
	env.fog_enabled = true
	env.fog_mode = Environment.FOG_MODE_DEPTH
	# Im Tiefenmodus ist die Dichte die Höchststärke (Standard 0,01, also fast kein Nebel).
	env.fog_density = 1.0
	env.fog_light_color = FOG_COLOR
	env.fog_depth_begin = fog_start
	env.fog_depth_end = fog_end
	env.fog_depth_curve = 1.0
	var node := WorldEnvironment.new()
	node.environment = env
	add_child(node)


func _add_light() -> void:
	var sun := DirectionalLight3D.new()
	sun.rotation = Vector3(-0.72, 2.2, 0.0)
	sun.light_energy = 2.0
	sun.shadow_enabled = true
	sun.directional_shadow_max_distance = 250.0
	add_child(sun)


func _add_water() -> void:
	var material := ShaderMaterial.new()
	material.shader = preload("res://shaders/water.gdshader")
	water = Node3D.new()
	# Folgt der Kamera in `_process`, nicht im Physik-Takt: nicht interpolieren.
	water.physics_interpolation_mode = Node.PHYSICS_INTERPOLATION_MODE_OFF
	water.position.y = world.sea_level()
	add_child(water)
	# Inneres Feld, dann vier Rechtecke als Ring (ohne Überlappung, sonst doppelt durchsichtig).
	var half_inner := WATER_INNER / 2.0
	var half_outer := WATER_OUTER / 2.0
	var band := half_outer - half_inner
	_add_water_plane(material, Vector2(WATER_INNER, WATER_INNER), Vector3.ZERO, WATER_INNER_SUBDIVISIONS)
	for side in [-1.0, 1.0]:
		_add_water_plane(material, Vector2(WATER_OUTER, band), Vector3(0, 0, side * (half_inner + band / 2.0)), 0)
		_add_water_plane(material, Vector2(band, WATER_INNER), Vector3(side * (half_inner + band / 2.0), 0, 0), 0)


func _add_water_plane(material: Material, size: Vector2, offset: Vector3, subdivisions: int) -> void:
	var plane := PlaneMesh.new()
	plane.size = size
	plane.subdivide_width = subdivisions
	plane.subdivide_depth = subdivisions
	var instance := MeshInstance3D.new()
	instance.mesh = plane
	instance.material_override = material
	instance.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	instance.position = offset
	water.add_child(instance)


func _physics_process(delta: float) -> void:
	# Im Physik-Takt wie die Spieler-Bewegung (Physik-Interpolation glättet die Kamera dazwischen).
	if player != null and autopilot != 0.0:
		player.position.x += autopilot * delta
		player.position.y = world.height_at(player.position.x, player.position.z) + 32.0


func _process(_delta: float) -> void:
	if player == null:
		return
	# Wasser folgt der Kamera, auf 64 m gerundet (Vielfaches des Rasters, kein "Mitzittern").
	water.position.x = roundf(player.position.x / 64.0) * 64.0
	water.position.z = roundf(player.position.z / 64.0) * 64.0


func _take_screenshot_later(path: String, frames: int) -> void:
	# Headless wird nichts gezeichnet (`frame_post_draw` käme nie).
	if DisplayServer.get_name() == "headless":
		push_error("Screenshot braucht ein Fenster (nicht --headless)")
		get_tree().quit(1)
		return
	for i in frames:
		await get_tree().process_frame
	await RenderingServer.frame_post_draw
	var image := get_viewport().get_texture().get_image()
	# Ohne echte Fenstergröße wäre das Bild 1×1 (wie #34 im Viewer): warten und neu versuchen.
	var retries := 0
	while image.get_width() <= 1 or image.get_height() <= 1:
		retries += 1
		if retries > SHOT_RETRIES:
			push_error("Screenshot: Fenster hat keine Größe")
			get_tree().quit(1)
			return
		push_warning("Screenshot ist nur %d×%d Pixel, neuer Versuch" % [image.get_width(), image.get_height()])
		for i in 30:
			await get_tree().process_frame
		await RenderingServer.frame_post_draw
		image = get_viewport().get_texture().get_image()
	var error := image.save_png(path)
	if error != OK:
		push_error("Screenshot nicht gespeichert: %s (%s)" % [path, error_string(error)])
		get_tree().quit(1)
		return
	print("Screenshot: %s" % path)
	get_tree().quit()
