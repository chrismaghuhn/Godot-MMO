// Stilisiertes Wasser: Tiefenfarbe in Stufen, Uferschaum, Wellen und Fresnel.
// Ohne Texturen: Noise wird im Shader berechnet.
//
// Benutzung: PlaneMesh mit genug Unterteilungen (für die Wellen, z. B. alle 1-2 m ein Vertex)
// auf Meereshöhe (`sea_level`). Flüsse sind bis unter Meereshöhe eingegraben und werden von
// derselben Fläche gefüllt.

shader_type spatial;
render_mode blend_mix, depth_draw_always, cull_disabled, specular_schlick_ggx;

group_uniforms farbe;
uniform vec3 shallow_color : source_color = vec3(0.25, 0.78, 0.76);
uniform vec3 deep_color : source_color = vec3(0.05, 0.22, 0.42);
/// Ab dieser Tiefe (m) ist die volle Tiefenfarbe erreicht.
uniform float depth_range : hint_range(0.5, 30.0) = 5.0;
/// Anzahl Farbstufen; 0 = weicher Verlauf.
uniform int color_steps : hint_range(0, 8) = 4;
uniform float shallow_alpha : hint_range(0.0, 1.0) = 0.55;
uniform float deep_alpha : hint_range(0.0, 1.0) = 0.95;

group_uniforms schaum;
uniform vec3 foam_color : source_color = vec3(0.95, 0.98, 1.0);
/// Breite des Schaumbands am Ufer (m Wassertiefe).
uniform float foam_width : hint_range(0.0, 2.0) = 0.45;
uniform float foam_noise_scale = 0.6;
uniform float foam_speed = 0.35;

group_uniforms wellen;
uniform float wave_height : hint_range(0.0, 1.0) = 0.08;
uniform float wave_scale = 0.25;
uniform float wave_speed = 1.0;

group_uniforms fresnel;
uniform vec3 horizon_color : source_color = vec3(0.70, 0.86, 0.95);
uniform float fresnel_power : hint_range(0.5, 8.0) = 3.0;
uniform float fresnel_strength : hint_range(0.0, 1.0) = 0.6;
group_uniforms;

uniform sampler2D depth_tex : hint_depth_texture, repeat_disable, filter_nearest;

varying vec2 world_xz;

float hash(vec2 p) {
	return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453);
}

float value_noise(vec2 p) {
	vec2 i = floor(p);
	vec2 f = fract(p);
	vec2 u = f * f * (3.0 - 2.0 * f);
	return mix(
		mix(hash(i), hash(i + vec2(1.0, 0.0)), u.x),
		mix(hash(i + vec2(0.0, 1.0)), hash(i + vec2(1.0, 1.0)), u.x),
		u.y);
}

// Eine Sinuswelle: addiert Höhe und Gradient (für die Normale).
void add_wave(vec2 p, float time, vec2 dir, float freq, float speed, float amp, inout float h, inout vec2 grad) {
	float phase = dot(dir, p) * freq + time * speed;
	h += amp * sin(phase);
	grad += amp * freq * cos(phase) * dir;
}

void vertex() {
	world_xz = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xz;
	vec2 p = world_xz * wave_scale;

	float h = 0.0;
	vec2 grad = vec2(0.0);
	add_wave(p, TIME, normalize(vec2(1.0, 0.3)), 1.0, 1.0 * wave_speed, 1.0, h, grad);
	add_wave(p, TIME, normalize(vec2(-0.4, 1.0)), 1.7, 1.3 * wave_speed, 0.6, h, grad);
	add_wave(p, TIME, normalize(vec2(0.7, -0.8)), 2.9, 1.9 * wave_speed, 0.3, h, grad);

	VERTEX.y += h * wave_height;
	// Gradient ist pro Einheit von p; zurück auf Meter umrechnen.
	grad *= wave_height * wave_scale;
	NORMAL = normalize(vec3(-grad.x, 1.0, -grad.y));
}

void fragment() {
	// Boden hinter dem Wasser aus der Depth-Textur (Godot 4.3+: Depth ist direkt NDC-z).
	float raw_depth = texture(depth_tex, SCREEN_UV).r;
	vec4 floor_view = INV_PROJECTION_MATRIX * vec4(SCREEN_UV * 2.0 - 1.0, raw_depth, 1.0);
	floor_view.xyz /= floor_view.w;

	// Senkrechte Wassertiefe statt Abstand entlang des Blicks, damit Schaum und Farbe
	// nicht vom Blickwinkel abhängen.
	float floor_y = (INV_VIEW_MATRIX * vec4(floor_view.xyz, 1.0)).y;
	float surface_y = (INV_VIEW_MATRIX * vec4(VERTEX, 1.0)).y;
	float water_depth = max(surface_y - floor_y, 0.0);

	float t = clamp(water_depth / depth_range, 0.0, 1.0);
	if (color_steps > 0) {
		t = floor(t * float(color_steps)) / float(color_steps);
	}
	vec3 col = mix(shallow_color, deep_color, t);
	float alpha = mix(shallow_alpha, deep_alpha, t);

	float fresnel = pow(1.0 - clamp(dot(NORMAL, VIEW), 0.0, 1.0), fresnel_power);
	col = mix(col, horizon_color, fresnel * fresnel_strength);
	alpha = clamp(alpha + fresnel * 0.3, 0.0, 1.0);

	// Schaum: hartes Band am Ufer, durch wandernde Noise aufgebrochen.
	float n = value_noise(world_xz * foam_noise_scale + vec2(TIME * foam_speed, TIME * foam_speed * 0.6));
	float foam = 1.0 - step(foam_width * (0.4 + n), water_depth);

	ALBEDO = mix(col, foam_color, foam);
	ALPHA = mix(alpha, 1.0, foam);
	ROUGHNESS = mix(0.12, 0.9, foam);
	SPECULAR = 0.5;
}
