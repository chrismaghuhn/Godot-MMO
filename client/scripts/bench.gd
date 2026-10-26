# Benchmark-Modus (wie #9 im Viewer): TERRAFORGE_BENCH=30 misst 30 s Frametimes und Job-Zeiten
# (nach 4 s Aufwärmen), gibt einen Bericht aus und beendet das Spiel. Typisch zusammen mit
# TERRAFORGE_AUTOPILOT=180. TERRAFORGE_BENCH_OUT=pfad schreibt den Bericht zusätzlich in eine Datei.
#
# Aussagekräftig nur mit sichtbarem Fenster und ohne andere GPU- oder CPU-lastige Programme.
extends Node

const WARMUP := 4.0

var terrain: TerraforgeTerrain
var seconds := 30.0
var elapsed := 0.0
var frames: Array[float] = []
## Zeit für das Umsetzen der Streaming-Events (Mesh-Upload, Nodes) pro Frame.
var uploads: Array[float] = []
var jobs := {"column": PackedFloat32Array(), "cave": PackedFloat32Array(), "far": PackedFloat32Array()}
var unfocused := 0


func _process(delta: float) -> void:
	elapsed += delta
	var times := terrain.take_job_times()
	if elapsed < WARMUP:
		return
	frames.append(delta * 1000.0)
	uploads.append(terrain.get_stats().get("upload_ms", 0.0))
	if not get_window().has_focus():
		unfocused += 1
	for key in jobs:
		jobs[key].append_array(times[key])
	if elapsed >= WARMUP + seconds:
		_report()
		get_tree().quit()
		set_process(false)


static func percentile(sorted: Array, p: float) -> float:
	return sorted[mini(int(sorted.size() * p), sorted.size() - 1)]


static func summary(values: Array) -> String:
	if values.is_empty():
		return "keine"
	var sorted := values.duplicate()
	sorted.sort()
	var sum := 0.0
	for v in values:
		sum += v
	return "Ø %.2f ms  p95 %.2f  max %.2f" % [sum / values.size(), percentile(sorted, 0.95), sorted[-1]]


func _report() -> void:
	var sorted := frames.duplicate()
	sorted.sort()
	var slow := frames.filter(func(ms): return ms > 20.0).size()
	var very_slow := frames.filter(func(ms): return ms > 33.0).size()
	var sum := 0.0
	for ms in frames:
		sum += ms
	var lines := [
		"BENCH %d s, %d Frames (Godot)" % [seconds, frames.size()],
		"  Frame: Ø %.2f ms  p50 %.2f  p95 %.2f  p99 %.2f  max %.1f  | > 20 ms: %d  > 33 ms: %d" % [
			sum / frames.size(), percentile(sorted, 0.5), percentile(sorted, 0.95), percentile(sorted, 0.99), sorted[-1], slow, very_slow],
		"  Upload pro Frame: %s" % summary(uploads),
		"  Jobs Spalten %5d×  %s" % [jobs.column.size(), summary(Array(jobs.column))],
		"  Jobs Höhlen  %5d×  %s" % [jobs.cave.size(), summary(Array(jobs.cave))],
		"  Jobs Fern    %5d×  %s" % [jobs.far.size(), summary(Array(jobs.far))],
		"  Fenster ohne Fokus in %d von %d Frames" % [unfocused, frames.size()],
	]
	var report := "\n".join(lines)
	print(report)
	var out := OS.get_environment("TERRAFORGE_BENCH_OUT")
	if out != "":
		var file := FileAccess.open(out, FileAccess.WRITE)
		if file == null:
			push_error("Bericht nicht geschrieben: %s (%s)" % [out, error_string(FileAccess.get_open_error())])
		else:
			file.store_string(report + "\n")
