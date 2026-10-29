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
- **Nachtrag (#10, 2026-10-26):** Der Godot-Client `gdext/` ist deshalb ein **eigener
  Cargo-Workspace** (im Haupt-Workspace `exclude`), damit kann keine Feature-Vereinigung mehr
  passieren. Zusätzlich exportiert der Core `UNDERGROUND_COMPILED`, und der Client bricht bei
  `true` in jedem Build ab (auch Debug: Godot kann Debug-Builds ausliefern). Das Review fand,
  dass eine reine Release-Prüfung über `debug-assertions` im Profil oder per Umgebungsvariable
  umgangen werden kann.

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

## D-017: Bäume auf der Volumen-Oberfläche, gefällte Bäume, Regel für Clients
- Datum: 2026-10-26
- Status: angenommen

**Kontext:** Bäume wurden auf der Höhenkarte geplant und schwebten über Höhleneingängen und
gegrabenen Löchern (#7). Bäume sollen später gefällt werden (Axt, #21) und das gespeichert werden.

**Entscheidung:**
- Jeder Baum hat eine stabile ID (Spalten-Chunk + Zelle).
- `settle_tree` setzt den geplanten Baum auf die tatsächliche Oberfläche (an den vier Ecken der Zelle
  gesucht und interpoliert). Fehlt Boden, ist er zugeschüttet oder liegt etwas Festes darüber, entfällt
  der Baum.
- Gefällte Bäume sind Teil der Weltänderungen (`WorldEdits::fell_tree`).
- **Regel für Clients (D-005):** Baum-Menge = `settle(öffentlicher Plan, Dichte, die der Client für
  den Chunk hat)`. Der Server ist nur für die gefällten Bäume maßgeblich. Fernterrain-Bäume (#54)
  nutzen den ungefilterten öffentlichen Plan und werden ersetzt, sobald der Chunk ankommt. Es dürfen
  keine Informationen über entfernte Bäume über den Radius hinaus gesendet werden, in dem Chunks
  ohnehin aufgedeckt sind – sonst verraten sie Höhleneingänge.

- Jede Ecke der Baum-Zelle wird mit ihrer eigenen geplanten Höhe verglichen (± 1,5 m).

**Folgen:** Mit Höhlen entfallen ≈ 0,1 % der Bäume (an großen Eingängen ≈ 3 %), ohne Höhlen und
Spieler-Änderungen keiner (geprüft über 33.119 Bäume; erste Fassung verglich mit der Wurzelhöhe und
entfernte an Hängen 0,04 %).

## D-018: Cave Culling über Seiten-Verbindungen, Erz-Filter mit 26 Nachbarn
- Datum: 2026-10-26
- Status: angenommen

**Kontext:** Unter der Oberfläche wurden alle geladenen Höhlen-Chunks gezeichnet, auch völlig
abgeschlossene (#8, D-006). Außerdem musste der Server-Filter für Erze (#46, D-005/D-013) genau
festlegen, wann ein Erz "freiliegt".

**Entscheidung:**
- **Cave Culling:** Jeder Chunk speichert, welche seiner 6 Seiten durch Luft miteinander verbunden
  sind (`occlusion::face_links`, Samples `0..=32`, also inklusive der mit dem Nachbarn geteilten
  Lage). Eine Breitensuche von der Kamera aus geht nur durch verbundene Seiten und darf eine
  eingeschlagene Richtung nicht umkehren (wie Minecraft). Nur erreichte Höhlen-Chunks werden gezeichnet.
  Reine Luft und fehlende Daten gelten als offen (lieber zu viel zeichnen als Löcher), massive Chunks
  ohne Daten als geschlossen. Chunks jenseits der Suchgrenze (Höhlenradius + 2) bleiben sichtbar.
  Im Röntgenmodus und mit `TERRAFORGE_CULL=0` wird nicht gecullt.
- **Erz-Filter:** Ein Erz-Voxel liegt frei, wenn einer seiner **26** Nachbarn Luft ist. Das ist genau
  die Menge, die Surface Nets einfärben kann. Geprüft wird, dass Geometrie und Vertex-Materialien
  von vollem und gefiltertem Chunk identisch sind.

**Folgen:**
- Oberfläche: ≈ 11 von 129 Höhlen-Chunks sichtbar, in einer Höhle ≈ 35 von 175 (bzw. 91 von 269);
  Screenshots mit und ohne Culling sind pixelgleich.
- Clients erhalten ≈ 3,8 % der Erz-Voxel, alle höchstens √3 m von Luft entfernt, die sie ohnehin
  sehen. Das liegt im Rahmen von D-005.
- Review-Funde (weggecullte Wände an Nachbar-Höhlen, unsichtbare tiefe Schächte, fehlende
  Erzflächen durch 6er-Nachbarschaft, wachsende Job-Messwerte, veraltete Röntgen-Marker) – behoben.

## D-019: Schmieden als Simulation statt Rezept
- Datum: 2026-10-26
- Status: angenommen

**Kontext:** Schmieden soll ein tiefer Skill werden, kein flaches Rezept-System. Jedes Schwert
soll einzigartig sein und so aussehen, wie es geschmiedet wurde. Es sollen so wenig Assets wie
möglich entstehen. Details in [docs/design/schmieden.md](docs/design/schmieden.md).

**Entscheidung:**
- Eine Klinge ist ein Datenmodell aus Segmenten (Form, Kohlenstoff, Gefüge, Härte, Fehler, …).
  Jede Aktion verändert es; Werte und Aussehen werden daraus berechnet, nichts wird ausgewürfelt.
- Simulation und Werte in `core/` (deterministisch, Server und Client gleich). Godot erzeugt nur
  Mesh und Shader-Parameter aus den Daten, ohne Modelle oder Texturen für die Klinge.
- Fortschritt über Wissen und Wahrnehmung (genauere Temperatur, Warnungen), nicht über Prozentboni.
- Die Dauer wächst mit dem Material (von 2-5 Min. für Eisen bis 45+ Min. für Meisterwerke),
  durch engere Fenster und mehr Schritte statt Timer. Gemeisterte Schritte gehen per Routine schneller.
- Vorerst kein Totalverlust: Fehler machen die Klinge schlechter, sichtbar und nur teilweise
  reparierbar. Gespeichert als Schweregrad 0.0-1.0, damit später eine Bruch-Schwelle möglich ist.
- Gemeinsames Schmieden (Meister, Zuschläger, Heizer) wird im Datenmodell mitgedacht, umgesetzt
  aber erst mit Multiplayer (D-010).

**Alternativen:**
- Rezept + Skill-Level → feste Werte: einfach, aber flach, und alle Schwerter gleich.
- Modellierte Klingen-Assets mit Varianten: widerspricht dem Ziel, alles zu generieren, und
  Aussehen und Werte hängen nicht zusammen.
- Totalverlust bei Fehlern: spannender, aber frustrierend, solange das System neu ist.

**Folgen:** Das Datenmodell der Klinge ist das Herzstück und muss vor der Umsetzung festgelegt
werden. Prozedurale Optik braucht einen klaren Stil, sonst wirkt sie technisch. Ein Schwert wird
als wenige hundert Bytes übertragen statt als Mesh.

**Ergänzung (2026-10-26):**
- Gemischt aktiv: Grundform geplant, kritische Momente (Schweißhitze, Falten, Abschrecken,
  Anlassen) in Echtzeit.
- Datenmodell allgemein geplant (Klingen, Äxte, Speerspitzen, Werkzeuge, Rüstungsplatten),
  gebaut wird zuerst nur das Schwert.
- Metallurgie vereinfacht real, keine Magie zum Start; magische Erze bleiben später möglich.

**Ergänzung Datenmodell (2026-10-26):** Details in
[docs/design/schmieden-datenmodell.md](docs/design/schmieden-datenmodell.md). Korrigiert zwei
Punkte von oben:
- Segmente haben eine eigene Länge und werden beim Strecken geteilt bzw. zusammengelegt
  (Remeshing). Alle Bezüge laufen über die Position entlang der Klinge, nicht über den Index.
- Gefüge in Zonen: 3 Breitenzonen (Schneide, Mitte, Rücken) plus Mantel-Lage, damit Lehm-Härten
  und Verbundstähle wie San-Mai physikalisch funktionieren.
- "Server und Client gleich" gilt nicht mehr: Simulation in Festkomma, der Server ist
  autoritativ und schickt nach jeder Hitze den Zustand; Bitgleichheit braucht nur das
  Nachspielen auf dem Server.
- Ein Langschwert ist ≈ 1,8 KB statt "wenige hundert Bytes".
- Nachtrag: Zwei Darstellungen. Arbeitszustand in feinem `i32` (≈ 6 KB pro Langschwert,
  Snapshot pro Hitze), Fertigzustand grob (≈ 2 KB), gerundet wird genau einmal beim
  Fertigstellen. Sonst hätten leichte Schläge keine Wirkung und die Volumenerhaltung würde
  driften. Pro Segment wird das Volumen gespeichert, die Länge daraus abgeleitet.
- Werkstücke speichern eine Formel-Version, fertige Stücke frieren ihre Werte ein.
- Volles Aktions-Log nur während der Arbeit, danach Snapshots pro Hitze.

Alternativen: feste Segmentlänge (widerspricht dem Strecken), ein Gefügewert pro Segment
(Lehm-Härten nur kosmetisch), f32 mit Bitgleichheit auf allen Clients (dauerhafte Fehlerquelle,
bei server-autoritativem Modell unnötig).

## D-020: Charakter prozedural, First-Person mit echtem Körper
- Datum: 2026-10-26
- Status: angenommen

**Kontext:** Für Phase 5 (#24) muss feststehen, wie ein Spieler aussieht, wie er sich bewegt und
kämpft. Wie beim Schmieden (D-019) sollen so wenig Assets wie möglich entstehen. Details in
[docs/design/charakter.md](docs/design/charakter.md) und [docs/design/kampf.md](docs/design/kampf.md).

**Entscheidung:**
- Stil stilisiert und etwas kantig. Körper aus Formen pro Knochen eines gemeinsamen Skeletts,
  Mesh und Skin-Gewichte werden erzeugt, keine Modell- oder Textur-Assets.
- First-Person mit echtem Körper: Kamera im Kopf des eigenen Charakters, andere sehen denselben
  Körper in Third-Person.
- Animation hybrid: Grund-Bewegungen als Keyframes im Code, darüber prozedurale Schichten
  (Fuß-IK, Proportionen, Nachschwingen, Gewicht). Bewegung in `core/`, Animation in Godot.
- Kampf aktiv und körperbetont: richtungsbasierte Schläge, Waffen-Physik aus der
  Schmiede-Simulation, Parieren, Ausweichen, Ausdauer, Trefferzonen.
- Proportionen wirken im Kampf (Reichweite), ausgeglichen durch Trefferfläche, Ausdauer,
  Ausweichtempo und eine begrenzte Spanne.
- Herkunftsregion beim Start bestimmt Kultur, Startwissen und Startpunkt, nicht den Körper.
- Der Körper verändert sich langsam und begrenzt durch das, was man tut (Muskeln, Bräune,
  ortsgenaue Narben).
- Kein Klettern zum Start.

**Alternativen:**
- Ein Basis-Mesh mit Morphs: einfacher, aber ein Asset und weniger eigener Stil.
- Volumen/SDF-Charaktere: sehr flexibel, aber schwer schön zu machen.
- Schwebende First-Person-Arme: einfacher, aber Körper und Reichweite passen nicht zum Modell,
  das andere sehen.
- Komplett prozedurale Animation: ohne Keyframes, aber sehr schwer gut aussehen zu lassen.
- Proportionen nur optisch: fairer, aber Körper und Waffe hätten weniger Bedeutung.
- Klettern überall (BOTW): passt zu großen Regionen, aber viel Aufwand für den Start.

**Folgen:** Ohne Klettern müssen Steilhänge über Wege, Umwege oder Graben/Bauen überwindbar
sein. Nahkampf im Multiplayer braucht Lag-Kompensation auf dem Server (Phase 6). Clipping und
Kamera-Stabilisierung sind bei First-Person mit echtem Körper eigene Aufgaben.

**Ergänzung (2026-10-26):** Vereinfachte Hände mit Daumen plus einer Fingergruppe (wenige
Knochen); Greifposen werden prozedural erzeugt. Volle Finger wären in First-Person schöner, aber
deutlich mehr Animationsaufwand.

## D-021: Godot-Client mit lokalem Server, Streaming und Bewegung im Core
- Datum: 2026-10-26
- Status: angenommen

**Kontext:** Phase 2 bringt die Welt in den Godot-Client (#10-#14). Drei Dinge waren offen:
Nach D-005 darf der Client das Feature `underground` nicht enthalten, ohne Server gäbe es dort
also keine Höhlen und Erze. Das Streaming im Viewer (`viewer/src/streaming/`, ~1.300 Zeilen) ist
an Bevy gebunden. Und Bewegung soll nach D-003/D-020 später auf dem Server laufen.

**Entscheidung:**
- **Lokaler Server-Prozess schon in Phase 2 (#56):** `server/` (headless, mit `underground`) und
  `protocol/` (gemeinsame Nachrichtentypen). Der Client erzeugt die Oberfläche selbst, der Server
  liefert Untergrund-Chunks in der Nähe (Erze gefiltert nach D-018) und prüft Graben/Bauen.
  Transport vorerst TCP auf localhost mit längen-präfixierten Nachrichten hinter einer
  Schnittstelle; die Netzwerk-Bibliothek wird erst in Phase 6 gewählt (#25). Im Singleplayer
  startet Godot den Server als Kindprozess. Bewegung läuft in Phase 2 nur lokal.
- **Streaming als Chunk-Manager im Core (#55):** `core/src/streaming/` entscheidet, welche Chunks
  gebraucht werden, baut sie in Worker-Threads und gibt Events aus (umgesetzt als `ShowColumn`
  mit Chunks und Bäumen, `RemoveColumn`, `ShowCave`, `RemoveCave`, `CaveVisibility`, `ShowFar`,
  `RemoveFar`). Der Core kennt keine Entities oder Nodes; Viewer und Godot
  setzen die Events nur um. Aufbau nach D-011, eine Datei pro Aufgabe:
  `mod.rs` (API, `Config`), `wanted.rs` (gewünschte Chunks, reine Funktionen), `columns.rs`,
  `caves.rs`, `far.rs` (je eine Zustandsmaschine pro Chunk-Art), `culling.rs` (D-018),
  `jobs.rs` (einziger Ort mit Threads), `dirty.rs` (Änderungen → Neubau), `source.rs`
  (`trait UndergroundSource`: lokal im Viewer, übers Netz in Godot), `tests.rs`.
  Konstanten stehen in `Config` statt verstreut. Ohne `underground` kennt das Streaming nur den Trait.
- **Bewegung und Kollision im Core (#13):** kinematischer Controller mit Kollision gegen das
  Dichtefeld, damit der Server ihn in Phase 6 unverändert nutzen kann. Godot liefert Eingabe und Kamera.
- **Godot-Seite:** gdext-Crate mit einer Datei pro Godot-Klasse (Terrain, Bäume, Spieler, Netz,
  Debug). Hintergrund-Arbeit in Rust-Threads, Godot-Objekte nur im Hauptthread mit Upload-Budget
  pro Frame. Zuerst ein `MeshInstance3D` pro Chunk, RenderingServer nur, wenn Messungen es nahelegen.
  Vorerst nur Linux-Builds.
- **Umfang Phase 2** zusätzlich zu Chunks, Bäumen, Bewegung und Graben: Fernterrain (D-015),
  Cave Culling (D-018), Wasser-Shader, Debug-Overlay (Frametimes, Chunk-Zeiten wie #9).

**Alternativen:**
- Integrierter Server im selben Prozess (wie Minecraft): weniger Aufwand, aber die Netzwerk-Grenze
  wäre nicht echt getestet und der Untergrund im Client-Prozess.
- Phase 2 nur mit Oberfläche: am einfachsten, verschiebt das Problem aber nur nach Phase 3.
- Streaming im gdext-Crate neu schreiben: schneller begonnen, aber zwei Versionen, die auseinanderlaufen.
- `CharacterBody3D` mit Jolt: schneller spielbar, für den autoritativen Server aber neu zu bauen.

**Folgen:** Phase 2 wird größer, ein Teil von #25/#26 ist vorgezogen. Die Netzwerk-Grenze besteht
von Anfang an, Phase 6 baut darauf auf. Vor dem ersten Godot-Chunk steht ein Umbau des Viewers (#55),
der ohne Verhaltensänderung bleiben muss (Hash über alle Chunk-Meshes). Für Release-Builds des
Clients gilt weiter die Regel zur Feature-Vereinigung aus D-005.

**Umsetzung Streaming (#55, 2026-10-26):**
- `Streamer<M>`: pro Frame `update(kamera)` und `drain_events()`. Meshes wandelt eine Funktion des
  Aufrufers schon im Worker-Thread in das Engine-Format `M` um. Events pro Spalte statt pro Chunk,
  damit ein Neubau die alte Spalte in einem Schritt ersetzt (kein Flackern).
- Eigener Thread-Pool (`jobs.rs`, so viele Threads wie Bevys Pool: ein Viertel der Kerne, 1-4).
  Verworfene Jobs laufen nicht mehr an, veraltete Ergebnisse werden an der Job-Nummer erkannt.
  Ein Panic im Job wird erst weitergegeben, nachdem alle anderen Ergebnisse übernommen sind und
  der Job vergessen ist; fängt die Engine ihn (gdext), läuft das Streaming weiter (Review-Fund).
- `UndergroundSource` mit `PublicSurface` (nur Oberfläche) und, mit Feature, `Underground`.
  Ohne Höhlen-Quelle werden massive Chunks nur geladen, wenn sie verändert wurden.
- Viewer: `viewer/src/streaming.rs` (241 Zeilen) setzt nur noch Events um; größte Datei im
  Core-Streaming 285 Zeilen.
- Geprüft mit dem neuen Szenen-Hash (`TERRAFORGE_SCENE_HASH`): sechs Szenen (Spawn, kleiner
  Radius, Höhle, Graben, Röntgen, ohne Fernterrain/Culling) sind Entity für Entity gleich wie
  vorher. Benchmark (#9, 30 s Autopilot): Frames Ø 16,67 ms, p95 17,18 ms vorher wie nachher,
  Job-Zeiten gleich.

**Umsetzung gdext-Crate (#10, 2026-10-26):**
- godot-rust 0.5.5 mit `api-4-7`, genau passend zu Godot 4.7.2 (statt einer älteren API: Client
  und Bibliothek werden zusammen versioniert).
- `gdext/` als eigener Workspace (siehe D-005, Nachtrag). Build und erster Start in
  `client/README.md`. Erste Klasse `TerraforgeWorld` (Höhe, Biom, Meeresspiegel); Seeds außerhalb
  von `u32` ergeben `null` statt still eine andere Welt.
- Smoke-Test `gdext/tests/godot_smoke.rs`: startet Godot headless mit einem temporären Projekt,
  das genau die gerade gebaute Bibliothek lädt, und vergleicht mit dem Core. Geprüft, dass er
  einen falschen Wert erkennt (Standard-Ordner, `--release`, anderer `CARGO_TARGET_DIR`).
- Geprüft: Release- und Debug-Bibliothek enthalten keinen Untergrund-Code (Viewer zum Vergleich
  120 Treffer); mit eingeschaltetem Feature bricht der Build ab.
- Zwei Lockfiles (Haupt-Workspace und `gdext/`): `gdext/tests/lockfiles.rs` prüft, dass der
  Client für alle Abhängigkeiten des Cores dieselben Versionen nutzt, sonst entstünden still
  verschiedene Welten (Review-Fund).
- Bekannt: Der erste Import mit Extension stürzt beim Beenden ab (#57), danach läuft alles.

**Umsetzung Chunks und Streaming in Godot (#11, 2026-10-26):**
- `TerraforgeTerrain` (Node3D) nutzt den Core-Streamer mit `PublicSurface`. Dessen Worker wandeln
  die Meshes schon in Godot-Daten um (`MeshData`, reine Rust-Daten ohne Godot-Objekte), der
  Hauptthread baut daraus `ArrayMesh`-Nodes. Dreiecke werden umgedreht: Der Core liefert sie gegen
  den Uhrzeigersinn, Godot erwartet Vorderseiten im Uhrzeigersinn. Ein `MeshInstance3D` pro Chunk
  bzw. Fernkachel, eine `Node3D` pro Spalte.
- Budget pro Frame wie im Viewer über die Grenzen des Streamers. Gemessen (30 s Autopilot,
  180 m/s, Radius 8 mit Fernterrain): Upload Ø 0,4 ms, p95 1 ms pro Frame. Mit VSync p99
  16,67 ms; ohne VSync Ø 0,93 ms, p99 1,39 ms pro Frame.
- Frustum Culling macht Godot selbst: Blick zum Himmel 20 statt 333 Draw Calls.
- Startszene in `client/scripts/main.gd`: Spawnpunkt wie im Viewer (`core/src/spawn.rs`,
  verschoben aus dem Viewer), Tiefennebel bis vor die garantierte Fernsicht, Sonne, Wasser-Shader
  (feines 2-m-Feld um die Kamera plus flacher Ring bis zum Horizont), F3-Overlay, Benchmark wie #9.
- Tests in Godot (headless, mit temporärem Projekt): Streaming nah (genaue Spalten und
  Chunk-Positionen, Entfernen beim Weiterfliegen), Fernterrain (184 Kacheln, ersetzt beim Flug über
  ein Fernfeld), Hauptszene 300 Frames. Jeder Skriptfehler lässt die Tests fehlschlagen (Godot
  beendet sich bei Laufzeitfehlern mit Exit 0, Review-Fund). Gegenproben: Fehlendes Entfernen von
  Spalten oder Fernkacheln und ein Laufzeitfehler im Overlay werden erkannt.
- Review-Funde behoben: Nebel war praktisch aus (`fog_density` ist im Tiefenmodus die
  Höchststärke), der Terrain-Test lief wegen eines verdeckten Skriptfehlers am Ursprung,
  Fernkacheln galten vor dem Einfügen als fertig (`Stats` hat jetzt `far_ready`/`caves_ready`).

**Umsetzung Bäume (#12, 2026-10-27):**
- Pro Spalte und Baum-Mesh (7 Arten × 3 Varianten) ein `MultiMeshInstance3D` unter der Spalte;
  die 21 Meshes werden einmal gebaut (`gdext/src/trees.rs`). Lage wie im Viewer (skaliert, um die
  Hochachse gedreht), Material matt und beidseitig, baumfreier Kreis um den Spawnpunkt wie im
  Viewer (`spawn_clearing`, 17 m).
- Gemessen (30 s Autopilot, VSync): kein Frame über 16,7 ms, Upload unverändert Ø 0,4 ms.
- Headless speichert Godot keine Instanz-Transforms (Dummy-Renderer). Deshalb prüft der Godot-Test
  nur Anzahlen; Transforms prüft ein Rust-Test, den freien Kreis ein Core-Test.

**Umsetzung lokaler Server (#56, 2026-10-27):**
- `protocol/` (Nachrichten, Längen-Präfix mit postcard über TCP, `NetworkSource`) und `server/`
  (mit `underground`). Der Client meldet sich mit Protokoll-Version und öffentlichem Seed an; der
  Server antwortet auf Chunk-Anfragen (`Mesh` wie `density_chunk`, `Full` wie `materials_chunk`)
  in beliebiger Reihenfolge, Worker im Streamer warten auf ihre Antwort.
- Der Server filtert Erze (nur freiliegende, 26 Nachbarn, D-018) und schickt nur, was von der
  öffentlichen Oberfläche abweicht (`Diff` in Läufen, `Public`, `Empty`). Grund: Das Höhlen-Carving
  verändert auch tief im Fels unsichtbar Dichten, nur ein Viertel der Oberflächen-Chunks ist
  bitgleich; abweichend sind ≈ 16 % der Dichten und 2 % der Materialien.
- Godot startet den Server als Kindprozess (`--exit-with-client`, Port aus der ersten Zeile der
  Ausgabe) oder verbindet mit `host:port`. Fällt er aus, liefert `NetworkSource` die öffentliche
  Oberfläche: Das Spiel läuft ohne Höhlen weiter.
- Geprüft: Über das Netz kommen genau die gefilterten Chunks des Servers an; verborgene Erze
  verlassen den Server nicht; falscher Seed wird abgelehnt; Server-Ausfall hängt nicht. In Godot
  am Spawnpunkt 129 Höhlen-Chunks (12 sichtbar) und 7713 Bäume, genau wie im Viewer. Der
  Release-Client enthält keinen Untergrund-Code (Server zum Vergleich 29 Treffer).
- Gemessen (30 s Autopilot): mit Release-Server p99 16,75 ms; mit Debug-Server ruckelt es (66
  Frames über 33 ms), deshalb nimmt der Client den neueren Build. Spalten-Jobs warten jetzt auf
  den Server (Ø 16 ms statt 4 ms), halten aber Schritt.
- Review-Funde behoben: Ein Panic in einem Server-Job (z. B. Koordinaten weit außerhalb der Welt,
  im Debug-Build ein Überlauf) beendete einen Worker für immer; jetzt werden Positionen geprüft,
  Jobs abgesichert, und der Client bekommt in beiden Fällen `Public`. Der Server endet auch, wenn
  Godot vor der Verbindung abstürzt (Ende von stdin). Der Erz-Test prüft jetzt jedes Sample.
- Bewusst so: Antwortet der Server einmal 10 s lang nicht, bleibt der Client für die Sitzung bei
  der öffentlichen Oberfläche (kein Neuverbinden; schon gebaute Höhlen bleiben stehen).
- "In der Nähe" (D-005) heißt in Phase 2: Oberflächen-Chunks im ganzen Sichtradius (mit flachen
  Höhlen und dort freiliegenden Erzen, bis ~256 m bzw. 1 km), reine Höhlen-Chunks im Höhlenradius.
- Nicht enthalten (Phase 6, #25/#26): Der Server prüft nicht, ob angefragte Chunks in der Nähe des
  Spielers liegen (der Client kennt seine Position selbst, Bewegung ist noch nicht autoritativ),
  keine Begrenzung der Warteschlangen und keine Schreib-Timeouts, keine Kompression über den Diff
  hinaus, nur ein Client. Graben und Bauen über den Server kommt mit #14.

**Umsetzung Bewegung und Kollision (#13, 2026-10-27):**
- `core/src/movement/`: Kapsel (r 0,35 m, 1,8 m, Augen 1,65 m) aus drei Kugeln, Kollision gegen die
  Fläche `dichte == 0` des trilinear interpolierten Dichtefelds (Abstand ≈ `-dichte/|∇|`).
  Laufen 4,5 m/s, Rennen 8 m/s, Sprung ≈ 0,8 m, steiler als 45° ist Wand (D-020), Folgen des Bodens
  beim Hinablaufen, Teilschritte ≤ 0,2 m, `dt` und Fallgeschwindigkeit begrenzt. Godot liefert nur
  Eingabe und Kamera (`TerraforgeBody`, `client/scripts/player.gd`); Fliegen (F) bleibt ohne Kollision.
- Dichtefeld im Streamer (`streaming/field.rs`): Chunks im Umkreis von 2 Chunks um die Kamera (in
  der Ich-Perspektive der Spieler), aus den Mesh-Jobs übernommen oder mit eigenen Jobs
  (`JobKind::Field`) nachgeholt; sonst die öffentliche Höhe. Graben/Bauen macht die Chunks ungültig.
- Review-Funde behoben: Das Feld war ab ~180 m vom Spawn leer (Chunks entstehen am Rand der Sicht,
  außerhalb des Feldes) – jetzt eigene Jobs, geprüft mit Untergrund über 300 m Voxel für Voxel. An
  Wänden zitterte der Körper 7–15 cm pro Frame: Die Kante zwischen Boden und Wand ist im
  interpolierten Feld eine 45°-Rampe, die er immer wieder hinauflief; jetzt merkt er sich berührte
  Wände (bis er sich 15 cm von der Wandebene entfernt oder die Wand neben ihm endet) und gleitet
  daran entlang; bleibt nach dem Abzug weniger als 30 % der Geschwindigkeit, bleibt er stehen (sonst
  pendelte er in Innenecken). In zu niedrige Gänge ging er mit dem Kopf in den Fels; jetzt wird die
  waagerechte Bewegung zurückgenommen, wenn die Kapsel nicht passt. Graben direkt neben dem Körper
  schob ihn aus der Grube, weil der Chunk bis zum Neubau fehlte; jetzt gilt der alte Stand weiter.
  An Wänden endete ein Sprung nach 10 cm (die Kante zählte im Steigen als Boden); jetzt gibt es im
  Steigen keinen Boden.
- Godot: Physik-Interpolation an (sonst ruckelt die Kamera auf Monitoren über 60 Hz); die Kamera
  selbst ist nicht interpoliert und folgt der interpolierten Position, damit die Maus ohne
  Verzögerung wirkt. Der Test-Server
  der Godot-Tests liegt in einem eigenen Ordner, damit das Spiel weiter den Release-Server nimmt.
- Gemessen (30 s Autopilot, Release-Server, Fenster im Fokus): kein Frame über 20 ms; Feld-Jobs
  Ø 5 ms im Hintergrund.
- Offen: Bäume ohne Kollision (#58); Körper-Maße fest (später vom Charakter, #24); `sin`/`cos` sind
  nicht auf allen Plattformen bitgleich (für Vorhersage gegen den Server in Phase 6, #59). Bei
  schnellem Graben kann ein älterer Neubau einen neueren Feld-Stand kurz überschreiben (bräuchte
  eine Edit-Version pro Chunk, mit #14 erledigt). Das Spielgefühl der Steuerung ist nur von Hand
  prüfbar (#60).

**Umsetzung Graben und Bauen (#14, 2026-10-27):**
- Gemeinsamer Teil im Core (`edits/action.rs`): Werkzeug-Stufen, Radien (Graben 2 m, Bauen 1,5 m),
  Reichweite 8 m, Ziel per Strahl vom Auge, Prüfung „nicht in den eigenen Körper bauen“, welche
  Chunks danach neu zu laden sind. Server, Godot-Client ohne Server und Viewer nutzen ihn (der
  Viewer unverändert, Szenen-Hash gleich, Reichweite dort weiter 64 m).
- Protokoll 2: Der Client schickt nur `Edit { Auge, Blickrichtung, Dig{tool} | Build }`. Der Server
  bestimmt das Ziel im echten Untergrund, prüft Format, Werkzeug, Weltgrenze, Tempo (eine Aktion pro
  100 ms und Verbindung), Auge nicht im Fels und Körper, gräbt und antwortet mit `Edited` (Ausbeute,
  Chunks zum Neuladen). `Edited` kommt erst, wenn die Änderung gilt; jede danach gestellte
  Chunk-Anfrage enthält sie. Aktionen laufen auf dem Server nacheinander.
- Der Server hält die Änderungen und schickt geänderte Chunks fertig, mit dem Erz-Filter auf dem
  neuen Stand: Frisch freigelegte Erze werden sichtbar, die Chunks zum Neuladen reichen dafür einen
  Voxel weiter (26 Nachbarn). Der Client merkt sich nur, welche Chunks verändert sind
  (`WorldEdits::mark_touched`), und lädt sie vollständig vom Server.
- Edit-Version im Dichtefeld: Jeder Job merkt sich beim Start einen Zähler; Ergebnisse von Jobs, die
  vor der letzten Änderung eines Chunks gestartet sind, verwirft das Feld.
- Ohne Server gräbt der Client in der öffentlichen Oberfläche. Nach einem Ausfall des Servers wird
  nicht mehr gegraben (die Welt mischte sonst Server-Stand und Oberfläche).
- Review-Funde behoben: Graben aus einem Auge im Fels meldete in `too_hard` verborgene harte Voxel
  (Untergrund abtastbar, D-005); jetzt abgelehnt, und `too_hard` zählt nur Voxel, die danach an
  Luft grenzen. Nach einem Server-Ausfall grub der Client in öffentlichen Daten und mischte beide
  Stände. Chunks konnten unter dem Schreib-Lock erzeugt werden; Aktionen konnten sich überholen;
  Ziel und Chunks wurden nicht gegen die Weltgrenze geprüft. Zweite Runde: „Auge im Fels“ lehnte
  ~5 % der Aktionen dicht an Wänden ab (jetzt erst, wenn die ganze Zelle um das Auge fest ist); ein
  Fehler mitten im Anwenden ließ Client und Server auseinanderlaufen (jetzt `Failed` mit den
  Chunks zum Neuladen).
- Offen (Phase 6, #25): Die Augenposition meldet der Client selbst. Bis die Bewegung
  server-autoritativ ist, kann ein Client an beliebiger Stelle graben und über Annahme/Ablehnung
  abtasten, ob ein Punkt Luft ist. Aktionen laufen samt Chunk-Erzeugung unter einem Lock im
  gemeinsamen Worker-Pool; viele Aktionen an kalten Stellen bremsen das Streaming (mit #25 eigene
  Queue). Bäume fällen gibt es im Godot-Client noch nicht (im Viewer X; mit #21). Änderungen gehen
  beim Beenden verloren (Speichern mit #19).

## D-022: Phase 3, Spielkreislauf vor Biom-Inhalt
- Datum: 2026-10-27
- Status: angenommen

**Kontext:** Phase 3 bringt Ressourcen und Biome mit Inhalt (#15-#19, #48, dazu #54 und #58). Offen
waren die Reihenfolge, wie Materialeigenschaften entstehen, wo das Inventar lebt und wie die
ersten 30-40 Biome ausgewählt werden.

**Entscheidung:**
- **Reihenfolge:** zuerst der Kreislauf Sammeln → Behalten (D-010): #15 Materialeigenschaften →
  #18 Abbauen und Inventar (mit Bäume fällen im Godot-Client und #58 Kollision mit Stämmen) →
  #19 Speichern. Danach #16 Biom-Varianten mit Daten → #17 Biome ausbauen → #48
  Oberflächen-Hinweise → #54 Bäume im Fernterrain.
- **Eigenschaften beim Abbau berechnet (#15):** Grundwerte pro Material in `materials.ron`; beim
  Abbauen werden sie deterministisch nach Ort abgewandelt (Region, Provinz, Tiefe, Gestein), und das
  Item trägt die Werte. Bei Erzen rechnet das nur der Server (Provinzen, Reinheit und
  Spurenelemente sind geheim, D-013). Kein Wert pro Voxel im Dichtefeld.
- **Inventar auf dem Server (#18):** Der lokale Server führt das Inventar, Godot zeigt es nur an.
  Graben legt die Ausbeute hinein, Bauen verbraucht daraus. So stimmt es schon für Phase 6, und ein
  Client kann keine Ausbeute fälschen.
- **Biome (#17):** Claude schlägt eine Auswahl über alle Familien vor (mit Begründung), der Nutzer
  streicht oder tauscht.

**Alternativen:** Biom-Inhalt zuerst (schönere Welt, aber noch nichts zu tun); Werte pro Voxel
(genauer, aber viel mehr Speicher und Netzlast); feste Werte pro Material (widerspricht #15);
Inventar im Client (schneller gebaut, müsste für Multiplayer umziehen).

**Folgen:** Items sind mehr als „Material × Anzahl“: Sie tragen Herkunft und Werte, die
Grundlage für Schmieden (D-019, `materialien` mit Provinz, Reinheit, Spurenelementen). Zum
Stapeln im Inventar müssen die Werte gerundet werden (festzulegen mit #18). Das Protokoll wächst um
Inventar-Nachrichten.

**Umsetzung Materialeigenschaften (#15, 2026-10-27):**
- `materials.ron`: zu Härte und Dichte kommen Zähigkeit (0 spröde bis 1 zäh) und
  Wärmeleitfähigkeit (W/(m·K)), reale Richtwerte. Erze nennen ihre möglichen Spurenelemente mit
  höchstem Anteil (z. B. Eisenerz: Phosphor, Schwefel, Mangan, Nickel; Golderz: Silber, Kupfer).
- `core/src/properties.rs` (öffentlich): `WorldGen::mined_sample(voxel, material)` wandelt die
  Grundwerte ab: je Region und Eigenschaft ein fester Faktor (±10 %, an Grenzen stetig gemischt),
  innerhalb der Region Noise (±4 %, ≈ 200 m), mit der Tiefe härter und dichter (+3 % bzw. +2 % je
  100 m, bis 200 m). Ergebnis `MinedSample` mit Material, Region und Werten.
- `underground/mined.rs` (geheim): Erze bekommen Reinheit (schon aus #45), Provinz und
  Spurenelemente. Werte eines unreinen Stücks liegen zwischen taubem Gestein und Mineral (nach
  Reinheit gemischt); die Spurenelemente sind je Provinz und Erz fest, reinere Erze haben weniger.
- Der Viewer zeigt nach dem Graben die Werte des häufigsten abgebauten Materials. Im Godot-Client
  werden sie mit dem Inventar (#18) sichtbar; dort rechnet sie der Server.
- Die Werkzeug-Prüfung nutzt weiter die Grundhärte: Ob die Schaufel etwas schafft, soll nicht
  unsichtbar vom Ort abhängen. Spurenelemente zählen in % des gewonnenen Metalls (Silber im Gold
  ergibt Elektrum), bei Kohle des Brennstoffs.
- Review-Funde behoben: Das taube Gestein war immer der Platzhalter „Gestein“ (unreiner Goldsand
  wurde fast so hart wie Granit, reinerer leichter); jetzt das öffentliche Gestein an der Fundstelle
  bzw. das erste Wirtsgestein der Erz-Regel. Die Provinz stand als Rasterzelle im Ergebnis (aus
  Items hätten sich Lage und Grenzen der Provinzen ablesen lassen); jetzt eine undurchsichtige
  Kennung. Die Hashes der Spurenelemente hängen an Material-ID und festen Element-Nummern statt an
  der Reihenfolge der Enums. Mehrere Tests bewiesen ihre Aussage nicht (Vergleich einer Funktion
  mit sich selbst, zu weite Toleranzen, Testlinie fast ohne Regionsgrenze).
- Vorgabe für #18 (Review-Fund): Die Werte gelten für natürlich gewachsenes Material. Gebautes
  behält die Werte des Items, sonst ließe sich Erz durch Ablegen in einer reichen Provinz veredeln.

**Umsetzung Abbauen und Inventar (#18, 2026-10-27):**
- `core/src/inventory.rs`: Ein Item ist Material mit Herkunft und Werten (`MinedSample` aus #15).
  Gleiche Items stapeln: gleiches Material, gleiche Herkunft (undurchsichtige Provinz-Kennung bei
  Erzen, sonst Region) und bei Erzen gleiche Reinheitsstufe (5 %). Die Werte eines Stapels sind der
  nach Anzahl gewichtete Mittelwert, wie bei einem Haufen Erz. 40 Plätze, höchstens 999 je Stapel;
  was keinen Platz findet, geht verloren und wird gemeldet.
- `Holdings` (Inventar plus gebaute Voxel mit ihrem Item) im Core, genutzt vom Server und vom
  Godot-Client ohne Server: Graben bewertet natürliches Material einmal je Material und Aktion am
  Fundort, gebaute Voxel geben ihr Item zurück. Bauen verbraucht vom gewählten Platz, höchstens so
  viele Voxel, wie dort liegen (`place_sphere` mit Grenze).
- Protokoll 3: `Build { slot }`, `Fell`, Antworten `Dug { lost }` und `Felled`; nach dem `Welcome`
  und nach jeder ausgeführten Aktion schickt der Server den ganzen Inventar-Stand. Der Server hat
  vorerst ein Inventar (ein Spieler, D-021); gespeichert wird es mit #19.
- Bäume fällen (im Godot-Client mit X): `vegetation::targeted_tree` sucht mit denselben Regeln wie
  das Streaming (`column_trees`, aus dem Spalten-Streaming herausgezogen, der Viewer zeigt laut
  Szenen-Hash dieselben Bäume) den vordersten Baum vor dem Gelände; der Server fällt ihn und legt
  Holz ins Inventar (Kristallbäume geben Quarz), die Menge nach Größe. Neues Material „Holz“
  (Kategorie `Wood`).
- Godot: Zeile unten mit dem Baumaterial, I öffnet das Inventar mit den Werten der Stapel, 1-9 bzw.
  Mausrad wählen den Platz.
- Review-Funde behoben: Ein leerer Stapel ließ die späteren Plätze nachrücken, B baute dann
  unbemerkt mit dem nächsten Material; jetzt feste Plätze, und `Build` nennt das erwartete
  Material (der Server lehnt bei einem veralteten Stand ab, `Built` meldet, was gebaut wurde).
  Gefällte Bäume kamen bei einem neu verbundenen Client nicht an (jetzt `FelledTrees` nach dem
  `Welcome`). Der freie Kreis um den Spawn war dreimal festgelegt (Viewer, Godot-Export, Server);
  jetzt überall `SPAWN_CLEARING`. Fällen bei vollem Inventar fällte den Baum und verlor das Holz
  (jetzt abgelehnt); markiert wird die Spalte statt des Chunks am Fuß. Bewertet wird am Voxel nahe
  der Mitte statt an einer Ecke der Kugel; der Inventar-Stand entsteht unter demselben Lock wie die
  Aktion; Reinheit und Spurenelemente gehen gerundet zum Client. Die Inventarliste baute sich alle
  0,1 s neu auf (Scrollen unmöglich); bei offenem Inventar grub ein Klick daneben.
- Offen: Ohne Server prüft der Client beim Fällen weder Tempo noch „Auge im Fels“. Ein Panic mitten
  im Bauen (`Failed`) führt das Inventar nicht nach. Gebaute Voxel (`Holdings.placed`) müssen mit
  #19 gespeichert werden, sonst werden sie nach einem Neustart wie natürliche bewertet.

**Umsetzung Kollision mit Bäumen (#58, 2026-10-29):**
- Jeder Baum hat einen Stamm als senkrechte Kapsel (`movement::Trunk`), Radius und Höhe je Art
  und Variante aus `vegetation/geometry.rs` (Stamm bis zum Ansatz der Krone), skaliert wie der
  Baum. Kronen kollidieren nicht. `vegetation::trunk` baut ihn aus derselben `TreePlacement` wie
  das MultiMesh.
- Der Streamer hält die Stämme der geladenen Spalten (`trunks_near`); die Bewegung fragt einmal pro
  Schritt die Stämme in Reichweite ab und schiebt die drei Kugeln der Kapsel aus Gelände und
  Stämmen. `Streamer::fell_tree` nimmt einen gefällten Stamm sofort heraus (nicht erst mit dem
  Neubau der Spalte); der Godot-Client nutzt es für Antworten des Servers, die Liste nach dem
  Verbinden und das Fällen ohne Server.
- Gefunden im Godot-Test: Das Wand-Gedächtnis aus #13 prüft 0,6 m hinter dem Körper, ob die Wand
  noch da ist; bei einem dünnen Stamm liegt dieser Punkt schon dahinter, der Körper vergaß den
  Stamm und kroch darum herum. Für Stämme wird jetzt knapp hinter der Körperoberfläche geprüft;
  der Core-Test läuft mit dünnen, mittleren und dicken Stämmen leicht neben der Achse.
- Review-Funde behoben: Kristallbäume hatten einen viel zu dünnen Stamm (der Spieler lief 1-1,5 m
  in die Spitzen); ein Test vergleicht jetzt jeden Stamm mit dem Querschnitt seines Meshes. Beim
  Bergablaufen (schon seit #13, durch Stämme öfter) schob das Nachführen am Boden den Körper auch
  hangabwärts, jeder zweite Schritt war 2-3× so lang; jetzt wird nur senkrecht nachgeführt, mit
  Test. Beim Herausschieben sieht jede Fläche die Lage nach der vorigen.
- Bewusst so: Läuft der Spieler fast frontal (weniger als ≈ 17°) auf einen Stamm zu, bleibt er
  stehen statt seitlich abzurutschen (`MIN_SLIDE` aus #13 gilt auch für Stämme); etwas schräger
  gleitet er vorbei.

**Umsetzung Spielstand (#19, 2026-10-29):**
- Der Server speichert (`server/src/save.rs`); die Datei enthält Geheimes (gegrabene Erze,
  Reinheit) und gehört nie zum Client (D-005). Eine Datei `world.tfsave` je Welt-Ordner: `postcard`
  mit Versionsnummer, öffentlichem Seed und einer Kennung des geheimen Seeds (Hash, verrät ihn
  nicht). Inhalt: Voxel-Änderungen je Chunk (damit sie sich später als Zeilen in die Datenbank aus
  #27 übernehmen lassen), gefällte Bäume, Inventar und gebaute Voxel mit ihrem Item.
- Geschrieben wird in eine Hilfsdatei, die danach umbenannt wird (ein Absturz beim Schreiben lässt
  den alten Stand stehen), ganz unter `edit_lock`. Ein Spielstand einer anderen Welt, einer anderen
  Version oder eine kaputte Datei wird nicht geladen; der Server startet dann nicht, statt ihn zu
  überschreiben.
- `terraforge-server --save ORDNER`: lädt beim Start, speichert alle 30 s, wenn sich etwas geändert
  hat, und beim Ende (Verbindung oder stdin zu). Godot gibt `user://saves/seed-<seed>` mit (Export
  `save_dir`) und beendet den Server geordnet: stdin schließen, bis zu 5 s warten, erst dann `kill`.
  Ohne Server wird nichts gespeichert. Den Ordner benennt nur der öffentliche Seed: Ändert sich der
  geheime (`TERRAFORGE_SECRET_SEED`), passt der Spielstand nicht mehr und wird nicht geladen.
- Nach der Anmeldung schickt der Server die veränderten Chunks (`TouchedChunks`): Sonst fragte ein
  Client nach einem Neustart Gebautes über der Oberfläche oder Schächte unter der Höhlengrenze gar
  nicht an.
- Gefunden beim Schreiben der Tests: Das Godot-Testprojekt heißt wie das Spiel und teilte sich
  dessen `user://`-Ordner, also auch den Spielstand. Jedes Testprojekt hat jetzt einen eigenen
  Ordner, der danach gelöscht wird; ein Test prüft das.
- Review-Funde behoben: Beim Beenden ging der letzte Stand verloren (Verbindung und stdin enden
  fast gleichzeitig; ein Thread sah „nichts zu tun“ und beendete den Prozess, während der andere
  schrieb; bei 2 MB Spielstand 15 von 15 Mal). Jetzt wartet jeder Aufrufer, bis die laufende
  Speicherung fertig ist; ein Test startet den echten Server mit großem Spielstand und schließt
  Verbindung und stdin wie Godot. Die Datei wird mit `sync_all` geschrieben (auch nach einem Absturz
  des Rechners ganz); geschrieben wird ohne `edit_lock` (nur der Stand wird darunter erfasst). Ein
  defekter oder fremder Spielstand führte still ins Spiel ohne Speichern; jetzt meldet der Server
  den Grund, Godot zeigt „Spielstand defekt“ und sperrt Graben und Bauen. Zwei Server auf demselben
  Ordner sperrt eine Lock-Datei aus. Godot startet den Server in eigener Prozessgruppe (Ctrl+C trifft
  nur Godot, der Server speichert). Der Spielstand führt die Materialnummern mit (umnummerierte
  Materialien laden nicht still falsch, neue dürfen dazukommen). Protokoll 4.
- Offen: Aktionen, die beim Ende noch in der Warteschlange liegen (vom Client nie bestätigt), fehlen
  im letzten Stand. Die ganze Datei wird alle 30 s neu geschrieben (≈ 9 Byte je geändertem Voxel;
  bis zur Datenbank in #27 vertretbar). Mit mehreren Spielern (#26) darf `TouchedChunks` nur die
  Umgebung des Spielers nennen, sonst verriete es weltweit, wo andere gegraben haben.

## D-023: Kampf-Prototyp vorziehen, mit Platzhaltern
- Datum: 2026-10-29
- Status: angenommen

**Kontext:** Der Spielkreislauf aus D-022 (#15, #18, #19) steht. Kampf bringt viel Spielgefühl,
hängt laut Plan aber an Schmieden (Phase 4, Waffen) und Charakteren (Phase 5, Körper und
Animation). Design: [docs/design/kampf.md](docs/design/kampf.md), D-020.

**Entscheidung:**
- Ein Kampf-Prototyp kommt jetzt, vor den Biomen (#16, #17): zuerst eine Trainingspuppe (#62),
  danach ein erster Gegner mit KI (#63).
- **Waffen:** feste Platzhalter Schwert, Axt, Speer, mit Länge, Masse und Schwerpunkt im selben
  Datenformat, das später das Schmieden füllt (D-019). Ausholzeit und Wucht folgen schon aus der
  Physik (Trägheit um den Griff), nicht aus festen Tabellen.
- **Ort:** Regeln und Schwung-Bahn im Core; Puppe, später Gegner und die Trefferprüfung auf dem
  lokalen Server (D-003), wie beim Graben. Client und Server rechnen dieselbe Bahn, die Anzeige
  folgt ihr.
- **Eingabe:** Kampfmodus mit R (Waffe ziehen/wegstecken). Darin ist die Maus gefangen und dreht
  die Kamera; linke Taste halten und die Maus bewegen wählt die Richtung (oben, links, rechts,
  Stich), loslassen schlägt; rechte Taste blockt. Außerhalb des Kampfmodus bleibt alles wie bisher
  (Linksklick gräbt).
- **Optik:** ohne Charaktermodell (#24): Waffe in First-Person als einfaches prozedurales Mesh,
  Puppe aus Primitiven.

**Alternativen:** Erst Schmieden und Charaktere bauen (richtig, aber lange ohne Kampf); Kampf nur im
Client rechnen (schneller, müsste für Phase 6 umziehen); feste Tasten je Richtung (leichter zu
lernen, weniger Gefühl); gleich mehrere Gegner (mehr Spaß, viel größer).

**Folgen:** Platzhalter-Waffen und Puppen-Optik werden ersetzt, wenn Schmieden und Charaktere
kommen; die Regeln und das Datenformat sollen bleiben. Die Biome rücken nach hinten.

