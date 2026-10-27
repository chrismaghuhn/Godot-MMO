# Godot-Client

Godot 4.7 (`project.godot`), bindet `terraforge-core` über den Rust-Crate
[`gdext/`](../gdext) ein (godot-rust, #10, D-021). Vorerst nur Linux.

## Bauen

`gdext/` ist ein **eigener Cargo-Workspace** (nicht Teil des Haupt-Workspaces). Aus dem
Repo-Hauptordner:

```bash
cargo build --manifest-path gdext/Cargo.toml            # Debug, lädt der Editor
cargo build --manifest-path gdext/Cargo.toml --release  # Release, für Export-Builds
```

Die Bibliothek landet in `gdext/target/debug/` bzw. `gdext/target/release/`;
[`terraforge.gdextension`](terraforge.gdextension) verweist dorthin. Nach einem neuen Build lädt
der Editor sie neu (`reloadable = true`).

**Warum ein eigener Workspace:** Cargo vereinigt die Features aller Crates, die ein Befehl baut.
Läge der Client mit Viewer oder Server in einem Workspace, käme über `--workspace` der geheime
Untergrund in den Client (D-005). Zusätzlich bricht jeder Build des Clients (Debug wie Release)
mit einer Fehlermeldung ab, falls `terraforge-core` doch mit `underground` gebaut wird.

## Erster Start nach dem Klonen

Godot kennt die Extension erst, wenn sie in `client/.godot/extension_list.cfg` steht (`.godot/`
ist nicht im Repo). Das Projekt einmal im Editor öffnen oder headless importieren:

```bash
godot --headless --path client --import
```

Dieser erste Import stürzt beim Beenden ab (#57), die Extension ist danach aber registriert; ab
dem zweiten Start läuft alles normal. Alternativ die Datei selbst anlegen:

```bash
mkdir -p client/.godot && echo "res://terraforge.gdextension" > client/.godot/extension_list.cfg
```

## Starten

```bash
cargo build --manifest-path gdext/Cargo.toml
cargo build -p terraforge-server --release
godot --path client
```

Höhlen und Erze kommen vom **lokalen Server** (`server/`, #56): Das Terrain startet ihn beim Start
als Kindprozess (der neuere Build aus `target/release` bzw. `target/debug`; der Debug-Build ist zum
schnellen Fliegen zu langsam) und beendet ihn mit dem Spiel. Ohne Server läuft alles, nur ohne
Höhlen (Warnung im Log). `TERRAFORGE_SERVER=off` schaltet ihn ab, `TERRAFORGE_SERVER=host:port`
verbindet mit einem laufenden Server (`terraforge-server --port 7777`). Den geheimen Seed liest der
Server aus `TERRAFORGE_SECRET_SEED`, sonst nimmt er den unsicheren Entwickler-Seed.

Die Startszene ([`scenes/main.tscn`](scenes/main.tscn), aufgebaut in
[`scripts/main.gd`](scripts/main.gd)) streamt das Terrain um die Kamera (`TerraforgeTerrain`,
#11), mit Fernterrain bis ~3,5 km, Nebel, Sonne und Wasser
([`shaders/water.gdshader`](shaders/water.gdshader)), Bäumen (MultiMesh pro Spalte, #12) und Höhlen
vom Server. Start am selben Spawnpunkt wie im Viewer.

- **Steuerung** (Ich-Perspektive, #13): WASD laufen, Leertaste springen, Shift rennen, F zwischen
  Laufen und Fliegen wechseln (Fliegen ohne Kollision, Q/E runter/hoch), rechte Maustaste halten
  zum Umsehen, F3 Overlay (Frametimes, Streaming, gezeichnete Objekte, Position, Biom). Bäume
  haben noch keine Kollision (#58).
- **Graben und Bauen** (#14, wie im Viewer): Linksklick gräbt eine Kugel (2 m) unter dem
  Fadenkreuz, B baut Erde (1,5 m), T wechselt das Werkzeug (Härte-Stufen, echte Werkzeuge mit
  #21). Reichweite 8 m. Mit Server schickt der Client nur Blick und Aktion; der Server bestimmt das
  Ziel im echten Untergrund, prüft Werkzeug, Tempo (höchstens alle 0,1 s) und ob man sich selbst
  einbaut, und schickt die geänderten Chunks. Ohne Server gräbt der Client in der öffentlichen
  Oberfläche. Das Ergebnis (Ausbeute je Material) steht im Overlay.
- **Umgebungsvariablen** (wie im Viewer): `TERRAFORGE_SEED`, `TERRAFORGE_VIEW_RADIUS`,
  `TERRAFORGE_FAR=0` (ohne Fernterrain), `TERRAFORGE_FLY` (fliegend starten), `TERRAFORGE_CAM_Y`
  (fliegend höher starten), `TERRAFORGE_PITCH`/`TERRAFORGE_YAW`
  (Grad), `TERRAFORGE_AUTOPILOT` (m/s nach +x), `TERRAFORGE_SHOT=pfad.png` mit
  `TERRAFORGE_SHOT_AFTER` (Frames) für einen Screenshot.
- **Benchmark** (wie #9 im Viewer, Fenster sichtbar lassen):

  ```bash
  TERRAFORGE_BENCH=30 TERRAFORGE_AUTOPILOT=180 godot --path client --resolution 1600x900
  ```

## Tests

```bash
GODOT=/pfad/zu/godot cargo test --manifest-path gdext/Cargo.toml
```

startet Godot headless mit [`tests/gdext_smoke.gd`](tests/gdext_smoke.gd) (Höhe und Biom im Vergleich
mit `terraforge-core`) und [`tests/terrain_smoke.gd`](tests/terrain_smoke.gd) (Streaming: Spalten,
Meshes, Entfernen beim Weiterfliegen, Höhlen vom Server), [`tests/player_smoke.gd`](tests/player_smoke.gd)
(Laufen und Springen) und [`tests/dig_smoke.gd`](tests/dig_smoke.gd) (Graben und Bauen mit und ohne
Server, neuer Stand auch in der Kollision). Der Test baut sich dafür ein temporäres Projekt, das genau die gerade
gebaute Bibliothek lädt; die des Editors bleibt unberührt. Ohne Godot (`GODOT` oder `godot` im
`PATH`) schlägt er fehl. Außerdem prüft `gdext/tests/lockfiles.rs`, dass `gdext/Cargo.lock` für
alle Abhängigkeiten des Cores dieselben Versionen hat wie das Haupt-`Cargo.lock` (sonst erzeugten
Client und Viewer/Server verschiedene Welten). Das Skript lässt sich auch direkt starten:

```bash
godot --headless --path client --script res://tests/gdext_smoke.gd -- 42 10 10
```
