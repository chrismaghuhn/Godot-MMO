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
godot --path client
```

Die Startszene ([`scenes/main.tscn`](scenes/main.tscn), aufgebaut in
[`scripts/main.gd`](scripts/main.gd)) streamt das Terrain um die Kamera (`TerraforgeTerrain`,
#11), mit Fernterrain bis ~3,5 km, Nebel, Sonne und Wasser
([`shaders/water.gdshader`](shaders/water.gdshader)). Start am selben Spawnpunkt wie im Viewer.
Bäume folgen mit #12, Höhlen mit dem Server (#56).

- **Steuerung:** WASD + Q/E fliegen, Shift schnell, rechte Maustaste halten zum Umsehen,
  F3 Overlay (Frametimes, Streaming, gezeichnete Objekte, Position, Biom).
- **Umgebungsvariablen** (wie im Viewer): `TERRAFORGE_SEED`, `TERRAFORGE_VIEW_RADIUS`,
  `TERRAFORGE_FAR=0` (ohne Fernterrain), `TERRAFORGE_CAM_Y`, `TERRAFORGE_PITCH`/`TERRAFORGE_YAW`
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
Meshes, Entfernen beim Weiterfliegen). Der Test baut sich dafür ein temporäres Projekt, das genau die gerade
gebaute Bibliothek lädt; die des Editors bleibt unberührt. Ohne Godot (`GODOT` oder `godot` im
`PATH`) schlägt er fehl. Außerdem prüft `gdext/tests/lockfiles.rs`, dass `gdext/Cargo.lock` für
alle Abhängigkeiten des Cores dieselben Versionen hat wie das Haupt-`Cargo.lock` (sonst erzeugten
Client und Viewer/Server verschiedene Welten). Das Skript lässt sich auch direkt starten:

```bash
godot --headless --path client --script res://tests/gdext_smoke.gd -- 42 10 10
```
