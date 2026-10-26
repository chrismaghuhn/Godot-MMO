# Design: Charakter, Bewegung und Kampf

Status: Entwurf (2026-10-26). Entscheidungen dazu: [D-016](../../DECISIONS.md#d-016-charakter-prozedural-first-person-mit-echtem-körper).

Gehört zu Phase 5 (Prozedurale Charaktere, #24). Kampf und Netzwerk betreffen auch Phase 6.

## Aussehen

Stil: **stilisiert und etwas kantig**, passend zu Voxel-Welt, Bäumen aus Primitiven und
Schwertern aus Segmenten. Keine Modelle oder Texturen als Assets.

### Aufbau aus Formen pro Knochen

- Alle Charaktere teilen ein **Grundskelett**.
- Jeder Knochen bekommt eine Form (Länge, Dicke, Profil). Das Mesh wird daraus erzeugt (Loft
  bzw. Kapseln), die Skin-Gewichte ergeben sich aus dem Abstand zum Knochen.
- **Körper:** Größe, Gliedmaßenlängen, Schulter- und Hüftbreite, Masse als Parameter.
  Die Spanne ist begrenzt (siehe Kampf).
- **Gesicht:** Nase, Ohren, Kinn, Brauen als parametrische Formen; Augen, Mund und Hautdetails
  im Shader.
- **Hände:** vereinfacht, Daumen plus eine Fingergruppe (wenige Knochen). Passt zum kantigen
  Stil; Greifposen für Hammer, Zange und Schwert werden prozedural erzeugt.
- **Haare:** aus Kurven erzeugte Strähnen oder Haarbänder, Frisuren als Parameter.
- **Ausrüstung:** Rüstung als Schale mit Abstand zur Körperoberfläche, passt dadurch auf jeden
  Körper und kann aus Schmiededaten entstehen ([schmieden.md](schmieden.md)).

### Erstellung

- Würfeln (Charakter-Seed) und anpassen im Editor.
- **Herkunftsregion beim Start** (Regionen aus D-014) bestimmt Kultur und Wissen, nicht den Körper:
  - Startkleidung und Muster im Stil der Region
  - Startwissen: Erze und Pflanzen der Heimat (Wahrnehmungs-Idee aus dem Schmieden)
  - Startpunkt in dieser Region
- Körper und Aussehen bleiben frei wählbar; die Region macht höchstens Vorschläge beim Würfeln.

### Der Körper zeigt, was man tut

- Schmieden → kräftigere Unterarme, Schwielen, Brandflecken
- Viel draußen in sonnigen Biomen → Bräune
- Viel getragen → breitere Schultern
- Treffer → Narben **an der Stelle**, an der man getroffen wurde
- Ruß und Schmutz, abwaschbar

Regeln: langsam (über Spielstunden) und mit Obergrenze; teilweise zurückbildend (Bräune verblasst,
Muskeln bauen langsam ab, Narben bleiben); Spieler können steuern (z. B. Narben beim Heiler
behandeln lassen).

## Perspektive: First-Person mit echtem Körper

- Die Kamera sitzt im Kopf des eigenen Charakters, nur der Kopf wird ausgeblendet. Keine
  schwebenden Extra-Arme: Man sieht die eigenen Hände, Arme und Füße.
- Andere Spieler sehen denselben Körper in Third-Person: **ein Modell für beides**.
- Augenhöhe aus der Körpergröße, Reichweite aus Armlänge plus Waffenlänge.
- Nötig: Kamera-Stabilisierung (gedämpftes Kopfwippen), Lösung gegen Clipping von Armen durch
  die Kamera, Spiegelungen (z. B. Wasser) und ein Charakterbildschirm, um sich selbst zu sehen.
- Schmieden in First-Person: eigene Hände, Zange und Werkstück im Blick.

## Bewegung und Animation (Hybrid)

- **Grund-Bewegungen** (Gehen, Laufen, Springen, Schwimmen, Schläge) als Keyframes **im Code**,
  ohne Animationsdateien, auf dem gemeinsamen Skelett.
- **Prozedurale Schichten darüber:**
  - Fuß-IK auf dem Terrain (Pflicht auf Voxel-Gelände)
  - Schrittlänge und Tempo passend zu den Proportionen
  - Nachschwingen von Haaren, Umhang, Waffe am Gürtel
  - Gewicht: kopflastige Waffen schwingen langsamer, schwere Ladung macht den Gang schwerfälliger
    (Werte aus der Schmiede-Simulation)
- Bewegung (Position, Kollision, Ausdauer) in `core/`, damit der Server sie später autoritativ
  prüft und der Client vorhersagt (D-003). Animation ist reine Darstellung in Godot.

### Bewegungs-Möglichkeiten zum Start

- Gehen, Laufen, Sprinten (Ausdauer), Ducken
- Springen
- Schwimmen
- Graben und Bauen (#14)
- **Kein Klettern** zum Start. Steile Stellen überwindet man über Wege, Umwege oder indem man
  Stufen gräbt bzw. baut. Klettern kann später dazukommen (z. B. mit Ausdauer und Griffigkeit je
  nach Gestein).
- Später denkbar: Gleiten, Reiten, Boote

## Kampf

Aktiv und körperbetont, eigenes Dokument: [kampf.md](kampf.md). Für den Charakter wichtig:
Reichweite aus Armlänge plus Waffenlänge, größere Charaktere haben eine größere Trefferfläche,
mehr Ausdauerverbrauch und langsameres Ausweichen.

## Offen

- Genaues Grundskelett (Anzahl Knochen). Festgelegt ist bisher die Hand (siehe Aufbau).
- Wie schnell und wie stark sich der Körper verändert
