# Entscheidungen

Hier werden wichtige Entscheidungen festgehalten, damit man später nachvollziehen kann,
**warum** etwas so gebaut ist. Entscheidungen werden nicht gelöscht: Wird eine ersetzt,
bekommt sie den Status "ersetzt durch D-xxx" und die neue verweist zurück.

Vorlage für neue Einträge:

```
## D-xxx: Titel
- Datum: JJJJ-MM-TT
- Status: angenommen | ersetzt durch D-yyy | verworfen

**Kontext:** Welches Problem, welche Rahmenbedingungen?
**Entscheidung:** Was wurde entschieden?
**Alternativen:** Was wurde verworfen und warum?
**Folgen:** Was bedeutet das für später (gut und schlecht)?
```

---

## D-001: Godot als Spiel-Client, Rust für Logik, Bevy nur noch als Debug-Tool
- Datum: 2026-10-26
- Status: angenommen

**Kontext:** Der Terrain-Prototyp ist in Rust/Bevy gebaut. Für das Spiel werden viel UI
(Inventar, Crafting, Chat), Skelett-Animationen und prozedurale Charaktere gebraucht.

**Entscheidung:** Der Client wird in Godot 4 gebaut. Die Spiellogik bleibt in Rust (`core/`)
und wird über gdext eingebunden. Der Bevy-Viewer bleibt als schnelles Debug-Tool für den Core.

**Alternativen:**
- Alles in Bevy: angenehm, wenn man nur Code schreiben will, aber UI und Animations-Werkzeuge
  müsste man weitgehend selbst bauen. Bevy ist vor 1.0, jede Version bringt Breaking Changes.
- Alles in Godot (GDScript/C#): Die Weltgenerierung müsste neu geschrieben werden und wäre
  für den Server nicht wiederverwendbar.

**Folgen:** Die Engine-Wahl betrifft nur den Client. Logik, die auch der Server braucht,
gehört immer in `core/`, nie in Godot-Skripte.

## D-002: Monorepo mit Cargo-Workspace
- Datum: 2026-10-26
- Status: angenommen

**Kontext:** Core, Viewer, Godot-Client und später Server teilen sich Code und ändern sich oft gemeinsam.

**Entscheidung:** Ein Repo. Cargo-Workspace mit `core/` (terraforge-core, ohne Bevy) und
`viewer/` (Bevy). Godot-Projekt in `client/`. Im Core wird `glam` in derselben Version wie in
Bevy genutzt, damit `Vec3`/`Quat` ohne Konvertierung passen.

**Alternativen:** Getrennte Repos: Änderungen am Core und am Client bräuchten koordinierte Versionen.

**Folgen:** `cargo run --release` im Root startet weiterhin den Viewer (`default-members`, Binary `terraforge`).

## D-003: Autoritativer Server in Rust
- Datum: 2026-10-26
- Status: angenommen

**Kontext:** Bei einem Spiel mit Wirtschaft ist Betrug durch manipulierte Clients das größte Risiko.

**Entscheidung:** Der Server entscheidet alles (Abbau, Crafting, Inventar, Handel). Clients
senden nur Eingaben. Der Server ist ein eigenständiges, headless Rust-Programm, das `core/` nutzt.

**Alternativen:** Client-autoritativ / Peer-to-Peer: einfacher, aber für eine Wirtschaft nicht vertretbar.

**Folgen:** Spiellogik muss so geschrieben sein, dass sie ohne Grafik läuft. Der Client kann
Aktionen vorhersagen (Prediction), das Ergebnis bestimmt aber der Server.

## D-004: Volumen-Terrain (Dichtefeld + Surface Nets) statt Höhenkarte
- Datum: 2026-10-26
- Status: angenommen

**Kontext:** Höhlen sind wichtig (Ressourcen, Erkundung, 26 Höhlen-Biome), und Spieler sollen
frei graben und das Gelände formen können. Eine Höhenkarte kann pro (x, z) nur eine Höhe haben:
keine Höhlen, keine Überhänge, kein Graben.

**Entscheidung:** Die Welt wird ein 3D-Dichtefeld in 32³-Chunks. Die bisherige Höhenfunktion
wird die Grundlage (`dichte = höhe(x,z) - y`), 3D-Höhlen-Noise wird abgezogen. Meshes entstehen
mit Surface Nets. Das wird vor dem Godot-Port umgesetzt, damit das Chunk-Streaming in Godot
nur einmal gebaut werden muss.

**Alternativen:**
- Höhenkarte + separate Höhlen-Meshes: wenig Umbau, aber kein freies Graben.
- Höhlen als eigene Bereiche (wie Dungeons): am einfachsten, aber keine zusammenhängende Welt.
- Marching Cubes statt Surface Nets: verbreitet, aber Surface Nets ist einfacher und liefert
  sauberere Meshes.

**Folgen:** Bestehende Biome, Flüsse und Berge bleiben erhalten. Pro Chunk gibt es deutlich mehr
Daten, deshalb werden leere Chunks übersprungen. LOD für weite Sicht ist schwieriger, für den
Anfang reichen Sichtweite + Nebel.

## D-005: Öffentliche Oberfläche, geheimer Untergrund (gegen X-Ray-Cheats)
- Datum: 2026-10-26
- Status: angenommen

**Kontext:** Ursprünglich sollte der Client die ganze Welt selbst aus dem Seed erzeugen (spart
Bandbreite). Dann kennt der Client aber jede Höhle und jede Erzader, und ein Cheat kann sie
anzeigen (X-Ray, bekannt aus Minecraft). In einem Spiel, in dem Ressourcen alles sind, würde das
die Wirtschaft zerstören. Vorbild ist das Prinzip von Valorants serverseitigem Fog of War:
Was der Client nicht weiß, kann ein Cheat nicht anzeigen.

**Entscheidung:** Die Generierung wird im Core getrennt:
- **Oberfläche** (Terrain-Form, Biome, Bäume): öffentlicher Seed, der Client erzeugt sie selbst.
- **Untergrund** (Höhlen, Erze, seltene Materialien): geheimer Seed, nur der Server erzeugt ihn.
  Der Client bekommt Untergrund-Chunks erst in der Nähe, Erze erst, wenn sie freiliegen (an Luft grenzen).

Dazu **Interest Management**: Clients bekommen nur Spieler, Monster und Items in ihrer Nähe.

**Alternativen:** Alles aus einem öffentlichen Seed: am wenigsten Bandbreite, aber X-Ray ist nicht verhinderbar.

**Folgen:** Mehr Bandbreite für Untergrund-Chunks (die sich gut komprimieren lassen, weil meist
massiver Fels). Die Trennung muss von Anfang an im Core angelegt werden, nachträglich wäre es ein
großer Umbau.

**Umsetzung (#3, 2026-10-26):**
- Cargo-Feature `underground` in `terraforge-core`; das Modul `core/src/underground/` wird nur
  damit kompiliert. Der öffentliche Teil muss ohne das Feature bauen, damit kann er technisch
  nichts aus dem Untergrund importieren. Geprüft: Ohne Feature enthält die Bibliothek keinen
  Untergrund-Code.
- `SecretSeed`: kein `Default`/`Display`, `Debug` zeigt `***`. `insecure_dev_seed` nur für
  Viewer und Tests. Der Viewer nimmt `TERRAFORGE_SECRET_SEED`, sonst den Entwickler-Seed.
- **Achtung, Feature-Vereinigung:** Cargo vereinigt Features aller Crates, die in *einem* Befehl
  gebaut werden. Ein Release-Build des Spiel-Clients muss deshalb einzeln laufen
  (`cargo build -p <client-crate> --release`), nie zusammen mit Viewer oder Server
  (`--workspace`). Sonst wäre der Untergrund im Client enthalten.

## D-006: Rendering-Strategie, nur zeichnen was sichtbar ist
- Datum: 2026-10-26
- Status: angenommen

**Kontext:** Volumen-Terrain mit Höhlen erzeugt viel Geometrie, die meist verdeckt ist
(z. B. Höhlensysteme unter dem Spieler).

**Entscheidung:** Stufenweise, immer mit Messung:
1. Nur Oberflächen vermaschen (ergibt sich aus Surface Nets), leere Chunks überspringen.
2. Frustum Culling und Backface Culling (übernimmt die Engine).
3. Cave Culling: Pro Chunk speichern, welche Seiten durch Luft verbunden sind; Flood-Fill von
   der Kamera findet nur Chunks, die sichtbar sein können.
4. LOD für ferne Chunks.

**Alternativen:** Godots gebackenes Occlusion Culling: braucht vorab gebackene Occluder,
passt nicht gut zu prozeduraler, veränderbarer Welt.

**Folgen:** Rendering-Culling (Client, Performance) und Fog of War (Server, Sicherheit) sind
getrennte Mechanismen, beide werden gebraucht.

## D-007: Weniger hügeliges Terrain
- Datum: 2026-10-26
- Status: angenommen

**Kontext:** Das Terrain wirkte zu hügelig. Messung über 7 Seeds (4×4 km): nur ~1 % des Landes
flach (< 5°), ~62 % steiler als 30°. Es lag also nicht am Seed, sondern an der Höhenformel:
1. Die Grundform hatte 152 m Höhenunterschied in einem schmalen Übergang.
2. Die Berg-Maske lief linear aus und erzeugte Kämme auf ~70 % des Landes.
3. Die Detail-Noise war im Flachland am stärksten statt am schwächsten.

**Entscheidung:** Basishöhe als Kurve (Meeresboden → Schelf → Küstenebene → sanftes Inland),
eigene schärfere Maske für Bergkämme (`ridge_mask`), Detail-Noise abhängig von Rauheit, Ebenen
bei hoher Erosion. Temperatur leicht angepasst, damit die Biom-Verteilung ähnlich bleibt.

**Ergebnis:** ~16 % flach (statt 1 %), ~28 % steiler als 30° (statt 62 %); steile Flächen fast nur
noch in Gebirgen. Biome: Berge 22 % → 15,5 %, Küste 3 % → 8,5 %, Tropen 11 % → 17 % (Seed 42, Landanteil).

**Folgen:** Gebirge wirken eher blockartig mit teils abruptem Rand, Flussufer sind weiterhin sehr steil.

## D-008: Keine häufigen Fenstertitel-Updates
- Datum: 2026-10-26
- Status: angenommen

**Kontext:** Der Viewer ruckelte beim normalen Fliegen: ~115 Hänger à ~80 ms in 30 s, unabhängig
vom Terrain. Ursache: Der Fenstertitel wurde 4× pro Sekunde geändert (Biom, Höhe, Temperatur).
Unter i3 + picom + NVIDIA blockiert jede Titeländerung ~80 ms.

**Entscheidung:** Der Titel wird nur einmal gesetzt. Die Infos stehen im F3-Overlay.

**Ergebnis:** 4 Hänger über 20 ms in 30 s (statt ~115), max 24 ms (statt 81 ms), stabile 60 FPS.

**Folgen:** Allgemeine Lehre: Fenster-/Betriebssystem-Aufrufe nie pro Frame oder in hoher Frequenz.
Im Viewer gibt es zusätzlich `TERRAFORGE_FPS_CAP` und `TERRAFORGE_PRESENT`; der FPS-Cap wird
mit picom-VSync **nicht** empfohlen, weil sich beides gegenseitig stören kann.

## D-009: Biome erst ausgewählt ausbauen
- Datum: 2026-10-26
- Status: angenommen

**Kontext:** Alle 352 Biom-Ideen sind in `biomes.ron` eingetragen. Aktuell sind Varianten nur
Namen mit Farbton, die Variante innerhalb einer Familie wird zufällig gewählt.

**Entscheidung:** Varianten bekommen Daten (Bedingungen, Vegetation, Ressourcen, Terrain-Eigenheiten)
und werden nach Bedingungen platziert. Zuerst werden 30-40 Biome vollständig ausgebaut, die anderen
behalten ihren Farbton und kommen nach und nach dazu. `biomes.ron` bleibt die einzige Quelle der Biom-Liste.

**Alternativen:** Alle 352 auf einmal: zu viel Inhalt auf einmal, keines würde richtig fertig.

**Folgen:** 8 Namen kommen in mehreren Kategorien doppelt vor und sollten beim Ausbau
eindeutig benannt werden.

## D-010: Reihenfolge Singleplayer → Multiplayer → MMO
- Datum: 2026-10-26
- Status: angenommen

**Kontext:** Ein MMO ist für eine Person extrem aufwendig (Netzwerk, Persistenz, Skalierung, Anti-Cheat).

**Entscheidung:** Zuerst muss der Kreislauf Sammeln → Craften → Verbessern allein Spaß machen.
Erst dann Multiplayer mit wenigen Spielern, erst danach MMO-Skalierung. Der Server wird aber von
Anfang an mitgedacht (Logik in `core/`, D-003).

**Folgen:** Nach Phase 4 gibt es einen bewussten Entscheidungspunkt.

## D-011: Code-Struktur, ein Modul pro Thema
- Datum: 2026-10-26
- Status: angenommen

**Kontext:** `viewer/src/main.rs` (813 Zeilen) und `core/src/vegetation.rs` (853 Zeilen) waren
gewachsen und mischten mehrere Themen (Streaming, Kamera, Overlay, … bzw. Platzierung und Geometrie).
Große Dateien machen Änderungen unübersichtlich und Konflikte wahrscheinlicher.

**Entscheidung:**
- Ein Modul pro Thema. Wird ein Thema größer, wird es ein Ordner mit `mod.rs` und Untermodulen
  (Beispiel: `core/src/vegetation/`).
- Richtwert: **~400 Zeilen pro Datei**. Kein hartes Limit, aber ab da wird geprüft, ob sich
  ein Thema abspalten lässt.
- Jede Datei beginnt mit einem `//!`-Kommentar, der sagt, wofür sie zuständig ist.
- Im Viewer ist jedes Thema ein eigenes Bevy-Plugin; `main.rs` steckt nur die Plugins zusammen.
- Sichtbarkeit so eng wie möglich (`pub(super)` für interne Bausteine, Methoden statt öffentlicher Felder).
- Umbauten ohne Verhaltensänderung bekommen einen eigenen Commit und werden geprüft
  (bei deterministischem Code: Ausgabe-Hash vorher/nachher vergleichen).

**Folgen:** Aufteilung am 2026-10-26: Viewer in `scene`, `streaming`, `meshes`, `controls`,
`overlay`, `devtools` (größte Datei 281 Zeilen); Vegetation in `placement`, `geometry`,
`primitives`, `rng` (größte Datei 364 Zeilen). Baumplanung und Baum-Geometrie sind
nachweislich unverändert (gleicher Hash über 3028 Bäume und alle Baum-Meshes).
