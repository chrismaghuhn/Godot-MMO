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

**Höhlen und Schutz-Schale (#4, 2026-10-26):**
- Höhlen dürfen die sichtbare Oberfläche nicht verändern (außer an Eingängen), sonst verrät
  die Oberfläche flache Höhlen. Regel: Luft wird nie verändert, und Samples weniger als 4 m
  unter der **niedrigsten Oberfläche der 3×3-Nachbarspalten** bleiben bitgenau gleich.
  Begründung: Ein Surface-Nets-Vertex hängt nur von den 8 Ecken seiner Zelle ab; so kann
  keine Zelle, die Luft berührt, verändert werden.
- Erste Version maß die Tiefe nur senkrecht. Das unabhängige Review (D-012) fand, dass dann
  an Steilhängen Oberflächen-Zellen verändert werden (8.462 Zellen in einer großen Stichprobe).
  Nach der Korrektur: 0 von 31,9 Mio. geprüften Zellen.
- Eingänge nur an Land (Oberfläche ≥ Meeresspiegel + 3 m), keine Höhlen unter y = -160.

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

## D-012: Unabhängiges Review vor Abschluss größerer Features
- Datum: 2026-10-26
- Status: angenommen

**Kontext:** Bei #4 (Höhlen) waren alle eigenen Tests grün, trotzdem enthielt die erste Version
einen Sicherheitsfehler (D-005: Oberfläche verriet Höhlen an Steilhängen). Die Tests prüften
die eigene Annahme, nicht die eigentliche Eigenschaft.

**Entscheidung:**
- Größere Features werden vor dem Schließen des Issues von einem **separaten Reviewer** (eigener
  Agent ohne den Kontext der Umsetzung) geprüft. Das Review ist nur lesend und bekommt
  Ziel, Anforderungen und Diff.
- Jeder Fund wird mit einem Test abgesichert, und es wird geprüft, dass dieser Test **ohne**
  die Korrektur fehlschlägt (Fehler als Mutation wieder einbauen).
- Nach den Korrekturen prüft der Reviewer nach.
- Tests sollen die eigentliche Eigenschaft prüfen (z. B. "keine Oberflächen-Zelle ändert sich"),
  nicht die Umsetzungsregel (z. B. "Samples mit Tiefe < 4 m ändern sich nicht").

**Folgen:** Mehr Aufwand pro Feature, dafür werden Fehler gefunden, bevor andere Teile darauf
aufbauen. Ergebnisse des Reviews werden im Issue und, wenn sie Entscheidungen betreffen, hier
festgehalten.

## D-013: Gestein und Erze, öffentlich vs. geheim, regionale Provinzen
- Datum: 2026-10-26
- Status: angenommen

**Kontext:** #5 (Gestein und Erze) ist die Grundlage für Ressourcen, Crafting und Handel.
Erze sind das Wertvollste in der Welt; ihre Positionen dürfen Clients nicht vorab kennen (D-005).

**Entscheidung:**
- **Öffentlich** (Client erzeugt selbst): Gesteinsschichten und Oberflächenmaterial (Erde, Sand,
  Kies, Schnee). Sie sind an Klippen, Hängen und Höhleneingängen sichtbar und müssen im Client
  korrekt aussehen. Sie verraten höchstens Wahrscheinlichkeiten, keine Positionen.
- **Geheim** (nur Server, Feature `underground`): Erze, ihre Reinheit, die Provinz-Einteilung,
  Oberflächen-Hinweise.
- **Materialien zum Start:** reale Erze (Kohle, Kupfer, Zinn, Eisen, Silber, Gold) plus
  Spezialmaterialien passend zu den Biomen (Quarz/Kristall, Obsidian, Schwefel, Magnetit, Zinnober).
  Datengetrieben in `core/assets/materials.ron`.
- **Stark regional:** Die Welt ist in geologische Provinzen geteilt (≈ 0,6-1,2 km). Jede Provinz
  hat nur wenige Erzarten, mindestens aber ein Grundmetall. Spezialmaterialien nur in passenden
  Biomen/Regionen (z. B. Obsidian/Schwefel vulkanisch, Quarz in Kristallgebieten).
- **Formen:** Nester (Klumpen), Adern (lang, dünn) und Seifen (angeschwemmt in Flusssand).
  Keine Flöze; Kohle kommt als Nester vor.
- **Reinheit** als langsam variierendes Feld pro Erz und Provinz (Grundlage für #15).
- **Schutzregel auch für Material:** Erze dürfen das Material von Zellen, die Luft berühren,
  nicht verändern (gleiches Argument wie bei der Höhlen-Schale). Seifengold liegt deshalb *im*
  Sand unter der sichtbaren Schicht.
- **Oberflächen-Hinweise** (verfärbtes Gestein, Aufschlüsse) gibt es, aber nur vom Server
  gesteuert, wie Höhleneingänge.
- Material-ID pro Voxel als `u16` (Platz für spätere, von Spielern gebaute Materialien).

**Alternativen:** Erze überall (kaum Handel), Gestein geheim (Client könnte Klippen nicht
korrekt darstellen), Flöze (bewusst weggelassen, um die Formen überschaubar zu halten).

**Folgen:** Aufteilung in Teil-Issues (5a-5f). Die Sichtbarkeits-Filterung (Erze erst senden,
wenn sie freiliegen) wird im Core vorbereitet und in #26 genutzt.

**Umsetzung Gestein (#44, 2026-10-26):**
- `core/src/geology.rs`: pro Spalte eine `ColumnGeology` (Bodenschichten, Basaltdecke,
  Schicht-Verschiebung und -Streckung, Grundgebirge, Marmorband), einmal in `ColumnData`
  berechnet; pro Sample nur `material_at(geologie, tiefe, y)`.
- Von oben: Boden je Biom → Basalt (vulkanisch) → Sedimente (unregelmäßige 85-m-Folge aus
  Sandstein/Schiefer/Kalkstein, gewellt, in Bergen gefaltet) → Marmorband → Granit
  (in Bergen bis ~150 m hochgedrückt).
- Hangneigung nur aus den Nachbarn in +x/+z: beidseitig ginge es für die Randspalte nicht,
  und Nachbar-Chunks müssen gemeinsame Samples bitgleich berechnen. Folge: leichte Richtungs-
  abhängigkeit, ab welcher Neigung der Boden fehlt.
- Werkzeug `geodump` (Viewer): Querschnitte und Oberflächenmaterial-Karte zum Prüfen.

**Umsetzung Erze (#45, 2026-10-26):**
- Provinzen: Voronoi mit ≈ 900 m Abstand, Mittelpunkte um 0,25-0,75 der Zelle versetzt (damit die
  3×3-Suche garantiert den nächsten findet). 1-5 Erzarten, immer ein Grundmetall; Spezialmaterialien
  nach Umgebung am Mittelpunkt. Quarz zusätzlich als Adern in Bergprovinzen, weil Kristall-Biome
  praktisch nie vorkommen (#49).
- Regeln als typisierte Tabelle (`ore_rules.rs`); Formen: Nester (3D-Noise über Schwelle),
  Adern (dünne Platten entlang einer Noise-Nullfläche, begrenzt durch zweite Noise), Seifen
  (vergrabene Goldsand-Linsen 4,5-8,5 m unter Flussbetten, weil die Flussboden-Schicht dünner
  als die Schutz-Schale ist). Noise auf dem gemeinsamen 4-m-Gitter (`grid.rs`).
- Abgestimmte Anteile am geeigneten Gestein (Seed 42): Eisen ≈ 3,8 %, Kohle ≈ 3,6 %,
  Kupfer ≈ 3,3 %, Zinn ≈ 1,8 %, Magnetit ≈ 1,5 %, Quarz ≈ 1,2 %, Silber ≈ 0,5 %, Gold ≈ 0,23 %.
  Erste Fassung hatte Gold/Silber als häufigste Erze (Adern zu dicht); per Messwerkzeug korrigiert.
- Erze nur ≥ `SURFACE_SHELL` unter der niedrigsten Oberfläche der 3×3-Nachbarschaft, ohne
  Ausnahme an Eingängen; getestet als Eigenschaft ("Material jeder Zelle an öffentlicher Luft
  bleibt"), auch an Eingängen. Erze enden bei y = -158, über `CAVE_MIN_Y`, weil Chunks darunter
  ohne Untergrund erzeugt werden (Review-Fund: Naht bei y = -160).
- `Underground::materials_chunk` liefert vollständiges Material auch für massiven Fels (für #6);
  `density_chunk` bleibt "braucht ein Mesh?". Akzentfarben in `materials.ron` für Erze.

## D-014: Große öffentliche Regionen (BOTW-Gefühl)
- Datum: 2026-10-26
- Status: angenommen

**Kontext:** Die Welt wirkte kleinteilig: Biom-Varianten wechselten alle ~100 m (Flickenteppich
aus Noise-Bändern), Familien-Flächen waren ~0,4 km groß. Wunsch: große Regionen mit klarer
Identität wie in Zelda Breath of the Wild (4-8 km). Zusätzlich kamen Kristall-Biome nie vor (#49).

**Entscheidung:**
- `core/src/regions.rs`: öffentliche Regionen als Voronoi-Zellen (Abstand 6 km, verzerrte
  Koordinaten für organische Grenzen). Pro Region: eine feste Variante je Biom-Familie, eine
  Klima-Tendenz und ein Thema (normal 81 %, Vulkan 7 %, Kristall 6 %, Pilz 6 %).
- Spezial-Biome kommen aus dem Regions-Thema statt aus eigenen Noise-Schwellen (löst #49).
- Übergänge: Jede nahe Region bekommt ein stetiges Gewicht nach Abstand, normiert. Gemischt
  werden Klima, Thema und Oberflächenfarbe über 700 m je Seite. Erste Fassung mischte nur zwei
  Regionen und sprang an Dreiländerecken (Review-Fund).
- Weltmaßstab 2-3× größer (Kontinente, Klima, Gebirge, Flüsse); Flussbreite und Uferneigung an
  die großräumigere Fluss-Noise angepasst.
- Regionen sind öffentlich (sichtbar), D-005 bleibt unberührt.

**Ergebnis (Seed 42, 32×32 km):** typische Varianten-Fläche 0,10 → ~1,5 km, Familien-Fläche
0,42 → ~1,6 km (Regionen selbst 4-8 km, darin mehrere Familien je nach Gelände). Mehr flaches
Land: < 5° ~16 % → ~33 %, steiler als 30° ~28 % → ~15 % (ergänzt D-007).

**Folgen:** Die ganze Welt hat sich verschoben (Spawn, Karten, Biom-Verteilung). Das Gefühl
großer Regionen braucht weite Sicht; dafür wird LOD (#33) wichtiger.

## D-015: Fernterrain (LOD) und weichere Gebirgsränder
- Datum: 2026-10-26
- Status: angenommen

**Kontext:** Seit D-014 sind Regionen 4-8 km groß, die Sicht endete aber nach ~250 m im Nebel (#33).
Außerdem stiegen Gebirge blockartig aus der Ebene auf (#36).

**Entscheidung (#33):**
- Fernterrain aus der öffentlichen Höhenkarte (`core/src/lod.rs`), Höhlen spielen in der Ferne keine
  Rolle. Zwei Stufen: 256-m-Kacheln mit 4-m-Raster bis ~1,5 km, 1024-m-Kacheln mit 16-m-Raster bis
  ~3,5 km. Schürzen an Kanten verdecken Risse zwischen den Auflösungen.
- Nahzone wird ausgespart, aber nur über vollständig geladenen Spalten; sonst liegt das Fernterrain
  leicht tiefer unter der Nahzone. So sieht man nie durch den Boden (Start, Teleport, schneller Flug).
  Aussparung mit Hysterese, Austausch von Kacheln erst, wenn der Ersatz fertig ist.
- Sichtradius der Nahzone höchstens 30 Chunks; Nebel endet vor der garantierten Reichweite (3072 m).
- Fernterrain hat noch keine Bäume (Folge-Issue).

**Entscheidung (#36):** Kamm-Maske setzt früher und weicher ein, dazu 30 m Vorgebirge, das Küste und
Schelf nicht anhebt.

**Zielkonflikt:** Weiche Ränder entstehen vor allem durch angehobenes Umland. Landanteil bleibt gleich
(Seed 42: 62,4 % → 62,9 %), aber mehr Land liegt über der Flussgrenze: Flussfläche ≈ -20 %,
Land über Meer + 55 m 25 % → 38 %, Biom "Berge" 13,5 % → 19 %, flaches Land 28 % → 24 %
(ergänzt D-007/D-014). Flüsse bis Meer + 75 m wurden getestet, machten die Täler aber wieder steil.

## D-016: Graben und Bauen über Änderungen pro Voxel
- Datum: 2026-10-26
- Status: angenommen

**Kontext:** Spieler sollen frei graben und bauen (D-004, #6). Später kommen Werkzeuge mit
Härte-Stufen (#21), Ausbeute fürs Inventar (#18), Speicherung (#19) und der Server (#25/#26).

**Entscheidung:**
- `core/src/edits.rs`: Änderungen werden pro **Welt-Voxel** gespeichert (Dichte + Material) und beim
  Bauen eines Chunks auf alle Chunks angewendet, die den Voxel als Sample enthalten. So bleiben
  Nähte nach Änderungen bitgenau.
- Graben/Bauen als Kugel-Distanzfeld (`min`/`max`), glatte Formen. Graben hat eine Härte-Grenze
  (Werkzeug) und liefert die Ausbeute pro Material (Voxel, die von fest zu Luft wurden ≈ m³).
- Der Generator wird über eine Funktion (`source`) angebunden, das Modul ist öffentlich.
- Viewer: Werkzeug-Stufen Schaufel 2,5 / Feuerstein 4,0 / Bronze 5,5 / Eisen 7,0 (Härte), Neubau
  betroffener Chunks im Hintergrund, altes Mesh bleibt bis zum Ersatz.

**Folgen und Hinweise:**
- **Server (D-005):** Änderungen allein reichen einem Client mit nur öffentlichen Daten nicht
  (Voxel in Höhlenluft oder zu hartem Gestein werden nicht gespeichert). Der Server muss für
  geänderte Chunks die fertigen Voxeldaten senden.
- Speicherung fehlt noch (#19), Bäume über Löchern schweben noch (#7).
- Review-Funde: verlorene Neubauten bei belegten Job-Plätzen, lange Locks beim Graben, Invariante
  im Speicher – behoben.
