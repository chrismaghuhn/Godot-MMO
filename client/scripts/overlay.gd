# Debug-Overlay (F3, wie im Viewer): Frametimes, Streaming, gezeichnete Objekte, Position, Biom,
# Werkzeug und letzte Grab-/Bau-Aktion. Das Fadenkreuz bleibt auch ohne Overlay sichtbar.
extends CanvasLayer

const FRAMES := 240

var terrain: TerraforgeTerrain
var world: TerraforgeWorld
var player: Node3D
var frame_times: Array[float] = []
var refresh := 0.25
var label: Label


func _ready() -> void:
	label = Label.new()
	label.position = Vector2(10, 10)
	label.add_theme_font_size_override("font_size", 14)
	label.add_theme_color_override("font_color", Color(0.55, 1.0, 0.55))
	var background := StyleBoxFlat.new()
	background.bg_color = Color(0.02, 0.03, 0.05, 0.72)
	background.set_content_margin_all(8)
	background.set_corner_radius_all(4)
	label.add_theme_stylebox_override("normal", background)
	add_child(label)
	var crosshair := Label.new()
	crosshair.text = "+"
	crosshair.add_theme_font_size_override("font_size", 22)
	crosshair.add_theme_color_override("font_color", Color(1, 1, 1, 0.85))
	crosshair.set_anchors_and_offsets_preset(Control.PRESET_CENTER)
	crosshair.grow_horizontal = Control.GROW_DIRECTION_BOTH
	crosshair.grow_vertical = Control.GROW_DIRECTION_BOTH
	add_child(crosshair)


func _mode() -> String:
	if player == null:
		return ""
	if player.flying:
		return "Flug"
	return "Lauf (am Boden)" if player.body.is_on_ground() else "Lauf (in der Luft)"


func _unhandled_input(event: InputEvent) -> void:
	if event is InputEventKey and event.pressed and not event.echo and event.physical_keycode == KEY_F3:
		label.visible = not label.visible


func _process(delta: float) -> void:
	frame_times.append(delta * 1000.0)
	if frame_times.size() > FRAMES:
		frame_times.pop_front()
	refresh += delta
	if not label.visible or refresh < 0.25:
		return
	refresh = 0.0

	var sorted := frame_times.duplicate()
	sorted.sort()
	var avg := 0.0
	for ms in frame_times:
		avg += ms
	avg /= frame_times.size()
	var p95: float = sorted[mini(int(sorted.size() * 0.95), sorted.size() - 1)]
	var s := terrain.get_stats()
	var camera := get_viewport().get_camera_3d()
	var position := camera.global_position if camera else Vector3.ZERO
	var objects := RenderingServer.get_rendering_info(RenderingServer.RENDERING_INFO_TOTAL_OBJECTS_IN_FRAME)
	var draw_calls := RenderingServer.get_rendering_info(RenderingServer.RENDERING_INFO_TOTAL_DRAW_CALLS_IN_FRAME)
	label.text = (
		"FPS %4.0f   Frame %5.2f ms   p95 %5.2f ms   max %5.2f ms   Upload %4.2f ms\n" % [Engine.get_frames_per_second(), avg, p95, sorted[-1], s.get("upload_ms", 0.0)]
		+ "Spalten %3d   Jobs %d+%d   Bäume %5d   Fern %3d (Jobs %d)   Höhlen %d (sichtbar %d)   Server %s\n" % [s.get("columns", 0), s.get("column_jobs", 0), s.get("columns_ready", 0), s.get("trees", 0), s.get("far_tiles", 0), s.get("far_jobs", 0), s.get("cave_meshes", 0), s.get("caves_shown", 0), s.get("server", "?")]
		+ "Job-Zeiten: Spalte %.2f ms   Höhle %.2f ms   Fern %.2f ms   Feld %.2f ms (%d Chunks)\n" % [s.get("column_job_ms", 0.0), s.get("cave_job_ms", 0.0), s.get("far_job_ms", 0.0), s.get("field_job_ms", 0.0), s.get("field_chunks", 0)]
		+ "Gezeichnet: %d Objekte inkl. Schatten, %d Draw Calls   (Terrain-Meshes %d)\n" % [objects, draw_calls, s.get("meshes", 0)]
		+ "X %7.1f   Y %6.1f   Z %7.1f   %s   Seed %d   [F3]\n" % [position.x, position.y, position.z, world.biome_at(position.x, position.z), world.seed()]
		+ "%s   [WASD] laufen  [Leertaste] springen  [Shift] rennen  [F] Laufen/Fliegen  [Q/E] im Flug runter/hoch  [rechte Maustaste] umsehen\n" % _mode()
		+ "Werkzeug %s   [Linksklick] graben  [B] bauen  [X] fällen  [T] Werkzeug  [I] Inventar   %s" % [TerraforgeTerrain.tool_name(player.tool), player.edit_message])
	label.add_theme_color_override("font_color", Color(0.55, 1.0, 0.55) if p95 <= 16.7 else (Color(1.0, 0.85, 0.35) if p95 <= 25.0 else Color(1.0, 0.45, 0.4)))
