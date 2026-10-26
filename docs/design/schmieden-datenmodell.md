# Design: Datenmodell des Werkstücks

Status: Entwurf (2026-10-26). Gehört zu [schmieden.md](schmieden.md), Entscheidungen in
[D-015](../../DECISIONS.md#d-015-schmieden-als-simulation-statt-rezept).

## Grundprinzip: Ursachen speichern, Wirkungen berechnen

Gespeichert wird, **was mit dem Stahl passiert ist** (Kohlenstoff, Härtungsgrad, Anlassen, Korn,
…). Härte, Zähigkeit, Schärfe, Masse, Balance und Aussehen werden **daraus berechnet**. So können
Werte und Optik nicht auseinanderlaufen.

## Aufbau

```
Werkstück
├── Kopfdaten (einmal pro Stück)
└── Zellen
    ├── Form     → je nach Art: Linie (Klinge, Speer, Axt) oder Fläche (Rüstungsplatte)
    └── Gefüge   → für alle Arten gleich
```

- **Linie:** Segmente entlang der Mittellinie, jedes mit eigener `laenge` (siehe Remeshing).
- **Fläche:** Raster aus Zellen mit demselben Gefüge-Teil, Form mit Dicke und Höhe/Wölbung pro
  Zelle (für Helme, Brustpanzer). Wird später gebaut, ist aber im Format vorgesehen.

## Kopfdaten

| Feld | Bedeutung |
|---|---|
| `id`, `seed` | Einzigartigkeit, Seed für Damastmuster und Noise |
| `art` | Schwert, Dolch, Axt, Speer, Platte, … |
| `materialien` | Bis zu 3 Ausgangsstähle: Erz-ID, Herkunftsprovinz, Reinheit, Spurenelemente (D-013) |
| `faltungen`, `verdrehungen` | Für das Damastmuster |
| `bereiche` | Erl, Ricasso, Klinge als **Positionen entlang der Klinge**, nicht als Segment-Index |
| `formel_version` | Version der Berechnungsformeln (siehe Werte einfrieren) |
| `geschichte` | Schmied, Mitwirkende, später Reparaturen und Besitzer |

## Segment (Linie)

### Form

| Feld | Bedeutung |
|---|---|
| `volumen` | Volumen des Segments. Die **Länge wird daraus abgeleitet** (Volumen ÷ Querschnittsfläche), damit die Volumenerhaltung exakt gilt |
| `mitte` (2 Werte) | Abweichung der Mittellinie: Krümmung in der Klingenebene, seitlicher Verzug |
| `breite` | Breite der Klinge |
| `dicke` | Dicke am Rücken |
| `schneide_dicke` | Dicke direkt an der Schneide |
| `fase_beginn` | Wo die Fase anfängt (0 = Schneide, 1 = Rücken) → Raute, Linse, flach |
| `hohlkehle` | Tiefe der Hohlkehle |
| `schneiden` | Einseitig oder zweiseitig |

Querschnitte sind Parameter statt fester Typen, damit alle Formen und Übergänge möglich sind.

### Gefüge: Zonen

Pro Segment gibt es **3 Breitenzonen** (Schneide, Mitte, Rücken) und **eine Mantel-Lage**
(Oberfläche):

- **Breitenzonen:** eigene `kohlenstoff`, `martensit` und Materialanteile. Damit ist Lehm-Härten
  physikalisch: Unter dünnem Lehm kühlt die Zone schneller ab und wird härter.
- **Mantel:** eigener `kohlenstoff`, Materialanteile und `entkohlung`. Damit gehen Verbundstähle
  durch die Dicke (San-Mai, Sandwich: harter Kern, weicher Mantel). Entkohlung ist eine
  Randschicht und gehört deshalb hierher.
- **Materialanteile:** Anteile der bis zu 3 Ausgangsstähle (zwei Werte, der dritte ergibt sich).

### Gefüge: pro Segment

| Feld | Bedeutung |
|---|---|
| `schichten` | Anzahl Lagen (durch Falten) |
| `schlacke` | Verunreinigung, nimmt durch Falten ab |
| `korn` | Korngröße fein ↔ grob |
| `anlass` | Höchste erreichte Anlasstemperatur |
| `lehm_grenze`, `lehm_dicke` | Wo und wie dick der Lehm lag |
| `spannung` | Innere Spannung |
| `riss`, `einschluss` | Fehler als Schweregrad 0-1; `einschluss` wird erst beim Schleifen sichtbar |
| `spuren_dichte`, `spuren_bahn` | Stärke der Hammerspuren und vorherrschende Hammerbahn |
| `spuren_hash` | Hash der Schläge auf dieses Segment, Seed für das Spurmuster |

**Abnutzung (später):** `schaerfe`, `scharten`, `rost`.

**Nur während der Arbeit:** `temperatur`. Fertige Stücke speichern sie nicht.

### Daraus berechnet

- Härte und Zähigkeit pro Zone aus Kohlenstoff, Martensit, Anlass und Korn
- Masse und Balance aus der Form
- Bruchfestigkeit aus Zähigkeit, Riss und Einschluss
- Schnitthaltigkeit aus der Härte der Schneidenzone und der Schneiden-Geometrie
- das gesamte Aussehen (siehe [schmieden.md](schmieden.md), "Alles prozedural")

## Zwei Darstellungen: Arbeits- und Fertigzustand

Würde der Zustand nach jedem Schlag auf grobe Werte gekürzt, hätten leichte Schläge unterhalb
einer Auflösungsstufe keine Wirkung, Rundungen würden in eine Richtung driften und die
Volumenerhaltung nach tausend Schlägen nicht mehr stimmen. Deshalb:

| Darstellung | Typ | Wann |
|---|---|---|
| **Arbeitszustand** | `i32`, feine Einheiten | Während des Schmiedens, im Speicher und als Snapshot pro Hitze |
| **Fertigzustand** | `u8`/`u16`, grob | Erst wenn das Stück fertig ist und in die Welt geht |

Gerundet wird **genau einmal**, beim Fertigstellen. Wird ein fertiges Stück später umgeschmiedet,
wird es wieder in den Arbeitszustand umgewandelt (einmaliger, kleiner Verlust).

Werte, die sich **nach** dem Fertigstellen noch in kleinen Schritten ändern (Abnutzung), brauchen
auch im Fertigzustand genug Auflösung (`u16`), sonst haben kleine Abnutzungen keine Wirkung.

### Wertebereiche (Entwurf)

Q16 = `i32` mit 16 Nachkommabits (für Werte 0-1). Zonen-Felder gibt es je Breitenzone und Mantel.

**Form**

| Feld | Einheit / Bereich | Arbeit | Fertig |
|---|---|---|---|
| `volumen` | mm³ | 0,001 mm³ | `u16`, 2 mm³ (max. ≈ 131 cm³) |
| `mitte` (2×) | ± mm | 1 µm | `i16`, 0,01 mm (± 327 mm) |
| `breite` | 0-300 mm | 1 µm | `u16`, 0,01 mm |
| `dicke` | 0-50 mm | 1 µm | `u16`, 0,01 mm |
| `schneide_dicke` | 0-5 mm | 1 µm | `u16`, 1 µm |
| `fase_beginn` | 0-1 | Q16 | `u8` |
| `hohlkehle` | 0-10 mm | 1 µm | `u16`, 0,01 mm |
| `schneiden` | Aufzählung | `u8` | `u8` |

**Gefüge je Zone** (3 Breitenzonen + Mantel)

| Feld | Einheit / Bereich | Arbeit | Fertig |
|---|---|---|---|
| `kohlenstoff` | 0-2,5 % | 0,0001 % | `u8`, 0,01 % |
| `martensit` | 0-1 | Q16 | `u8` |
| Materialanteile (2×) | 0-1 | Q16 | `u8` |
| `entkohlung` (nur Mantel) | Tiefe 0-2,5 mm | 1 µm | `u8`, 10 µm |

**Gefüge je Segment**

| Feld | Einheit / Bereich | Arbeit | Fertig |
|---|---|---|---|
| `schichten` | Anzahl | `u32` | `u16` (gekappt) |
| `schlacke` | 0-1 | Q16 | `u8` |
| `korn` | 0-1 (fein ↔ grob) | Q16 | `u8` |
| `anlass` | 0-700 °C | 0,1 °C | `u16`, 1 °C |
| `lehm_grenze` | 0-1 (über die Breite) | Q16 | `u8` |
| `lehm_dicke` | 0-10 mm | 1 µm | `u8`, 0,1 mm |
| `spannung` | 0-1 | Q16 | `u8` |
| `riss`, `einschluss` | 0-1 | Q16 | `u8` |
| `spuren_dichte` | 0-1 | Q16 | `u8` |
| `spuren_bahn` | Aufzählung | `u8` | `u8` |
| `spuren_hash` | Hash | `u32` | `u32` |
| `temperatur` | 0-1600 °C | 0,1 °C | (nicht gespeichert) |

**Abnutzung (nur Fertigzustand, ändert sich in kleinen Schritten)**

| Feld | Bereich | Fertig |
|---|---|---|
| `schaerfe`, `scharten`, `rost` | 0-1 | `u16` |

### Größe

- **Arbeitszustand:** ≈ 30 Werte à 4 Bytes ≈ 120 Bytes pro Segment, ein Langschwert
  (~50 Segmente) ≈ **6 KB** pro Snapshot.
- **Fertigzustand:** ≈ 40 Bytes pro Segment, ein Langschwert ≈ **2 KB**. Komprimiert deutlich
  weniger, weil benachbarte Segmente ähnlich sind.

## Remeshing

Beim Strecken wird Material länger (ein 15-cm-Knüppel wird zur 75-cm-Klinge). Deshalb:

- Segmente über **~3 cm** werden geteilt, unter **~1 cm** mit dem Nachbarn zusammengelegt.
- Das Volumen wird exakt aufgeteilt bzw. addiert, Gefügewerte werden **masse-gewichtet** gemittelt.
- Remeshing ist deterministisch (feste Regeln, feste Reihenfolge).
- Segment-Indizes verschieben sich dabei. Alles, was sich auf eine Stelle bezieht (Log, Bereiche,
  Muster), nutzt die **Position entlang der Klinge**.

## Regeln

### Hammerschlag

**Eingabe** (derselbe Befehl für Solo, Co-op und die geplante Grundform):

- Ziel: Position entlang der Klinge plus Stelle quer dazu (Schneide, Mitte, Rücken)
- Seite: Flachseite oder Schmalseite
- Stärke 0-1
- Hammer (Gewicht, Bahn: flach, ballig, Finne) und Ambossstelle (Fläche, Horn)

**Wirkung:**

1. **Verformbarkeit** aus Temperatur und Kohlenstoff. Unter ≈ 700 °C kaum Verformung, dafür
   steigen `spannung` und Rissgefahr.
2. **Volumen bleibt erhalten:** Verlorene Dicke wird Breite oder Länge. Finne quer streckt eher,
   flache Bahn breitet eher. Pro Hitze geht etwas Material als Zunder verloren.
3. **Nachbarn:** Der Schlag verteilt sich mit Abfall auf 1-3 cm je nach Hammergröße.
4. **Einseitig schlagen krümmt:** Eine dünner getriebene Schneide wird länger, die Klinge biegt
   sich. Schläge auf die Schmalseite stauchen und richten.
5. **Gefüge:** Schläge bei richtiger Temperatur verfeinern das Korn etwas.
6. **Abkühlung:** Jeder Schlag kühlt etwas.
7. **Spuren:** Starke Schläge, balliger Hammer und kalter Stahl erhöhen `spuren_dichte`; jeder
   Schlag fließt in `spuren_hash` ein.

**Geplante Grundform:** Ein Planer rechnet aus der Zielform die nötigen Schläge und führt sie
über genau diese Regel aus. Die Präzision hängt vom Wissen des Charakters und vom Material ab.
Es gibt nur eine Simulation.

### Wärme

- **Esse** heizt eine Zone: Position plus Breite des Feuers (je nach Esse).
- **Wärmeleitung** zwischen benachbarten Segmenten (1D-Diffusion).
- **Verlust** abhängig vom Verhältnis Oberfläche zu Volumen: Dünne Stellen kühlen schneller aus.
- Dadurch lässt sich z. B. die Spitze gezielt nachheizen, ohne den Rest zu überhitzen.

### Wärmebehandlung

| Vorgang | Regel |
|---|---|
| Abschrecken | Abkühlrate pro Zone aus Dicke, Lehm und Medium (Wasser schnell, Öl langsamer). Hohe Rate bei genug Kohlenstoff → viel `martensit`. Martensit dehnt sich aus, ungleiches Härten **krümmt** die Klinge (beim Katana gewollt). Zu hohe Rate in dicken oder spannungsreichen Zonen → `riss` steigt. |
| Normalisieren | Erhitzen, an der Luft abkühlen: `korn` feiner, `spannung` sinkt, `martensit` zurückgesetzt. |
| Anlassen | `anlass` = höchste erreichte Temperatur; Martensit wird zäher und weniger hart. |
| Feuer | Zeit × Temperatur → `entkohlung` im Mantel. Über ≈ 1150 °C wächst das Korn schnell. Beim Falten gleicht sich der Kohlenstoff zwischen den Lagen etwas an (Diffusion), `schlacke` sinkt. |
| Schleifen | Trägt Material ab, setzt `schneide_dicke` und Fase, entfernt Spuren und Entkohlung, kann Einschlüsse freilegen. |

## Rechnen, Netzwerk und Speicher

- **Festkomma** (i32/i64) in der Simulation. Der Zustand ist ohnehin ganzzahlig, und Logs sollen
  auch nach Jahren mit anderem Compiler und anderer Hardware gleich nachspielbar sein. Von Anfang
  an in Festkomma entwickeln (hinter einem eigenen Zahltyp `Fx` als Newtype mit Rechenoperationen),
  nicht erst in f32: Portierte Formeln verhalten sich sonst anders, vor allem bei Divisionen und
  kleinen Werten.
- **Server autoritativ:** Der Client sagt nur voraus. Der Server schickt nach jeder Hitze den
  korrigierten Arbeitszustand (≈ 6 KB). Clients müssen deshalb nicht bitgleich rechnen; Bitgleichheit
  braucht nur das Nachspielen auf dem Server.
- **Log:** Während der Arbeit speichert der Server das volle Aktions-Log (kompaktes Format, eine
  Klinge hat schnell tausende Schläge). Nach dem Fertigstellen bleiben nur **Snapshots pro
  Hitze bzw. Schritt** plus die Geschichte; das volle Log wird archiviert oder verworfen.

## Werte einfrieren

Formeln werden sich beim Balancing ändern. Damit ein Schwert nicht über Nacht schlechter wird:

- Jedes Werkstück speichert `formel_version`.
- Fertige Stücke **frieren ihre berechneten Werte ein**.
- Änderungen an alten Stücken nur bewusst, per Migration und angekündigt.

## Lesbarkeit

Jeder verborgene Wert braucht ein Signal, das man lesen kann. Wissen macht das Signal genauer.

| Verborgener Wert | Signal |
|---|---|
| Temperatur | Glühfarbe |
| Spannung, Risse | Klang beim Schlag (ein Riss klingt dumpf) |
| Kohlenstoff | Funkenbild beim Schleifen (Funkenprobe) |
| Anlassen | Anlassfarben |
| Härteverteilung | Hamon nach dem Ätzen |
| Einschlüsse | Erst beim Schleifen sichtbar |

Rezepte werden in Wikis landen. Das ist in Ordnung: Ein Rezept zu kennen ist nicht dasselbe,
wie es auszuführen. Die kritischen Momente sind aktiv, das Material schwankt je nach Provinz,
und man muss die Signale lesen können ("bei 800 °C abschrecken" hilft nur, wenn man 800 °C
erkennt).

## Grundform-Eingabe (Entwurf)

- **Vorlage wählen** (Langschwert, Säbel, Dolch, …) als Startpunkt.
- **Kontrollpunkte verschieben** in drei Ansichten:
  - Silhouette (Breite, Spitze)
  - Profil (Krümmung, Dicke am Rücken)
  - Querschnitt entlang der Klinge (Fase, Hohlkehle, Schneidendicke)
- Kein freies Skizzieren: mit Maus in First-Person schwer bedienbar, ergibt unsaubere Formen.
- **Materialcheck:** Die Zielform wird gegen das Volumen des Rohlings geprüft ("reicht nicht für
  diese Länge", "Spitze wird zu dünn"). Weil `volumen` gespeichert ist, geht das exakt.
- **Soll/Ist-Anzeige:** Der Planer erreicht die Zielform je nach Wissen nur ungefähr. Die
  Abweichung wird als Überlagerung gezeigt. Das macht das Wissen sichtbar und gibt einen Grund,
  von Hand nachzuarbeiten.
- **Wissen:** Anfänger haben wenige Vorlagen und grobe Kontrollpunkte. Meister haben mehr Punkte
  und können eigene Vorlagen speichern (eigene Handschrift, handel- und weitergebbar).

## Prototyp

Die konkreten Formeln werden nicht auf Papier festgelegt, sondern im Prototyp entwickelt und
justiert. Der Prototyp ist reiner Rust-Code in `core/`, ohne Engine. Er wird **in Phase 4**
gebaut wie geplant; zuerst kommt der Godot-Client (Phase 2).

Umfang:

- **Aktions-Enum** (Schlag, Erhitzen, Abschrecken, Falten, Schleifen, …) als ausdrückliches
  Ergebnis. Er wird für Test-Skripte gebraucht; das Log-Format ist später nur noch Serialisierung
  (#25).
- Hammerschlag, Wärme, **Remeshing** und **Abschrecken**. Die schwierigen Teile gehören in den
  Prototyp, nicht erst danach.
- Formeln zuerst einfach, Konstanten in einer Datei (z. B. `forging.ron`), damit sie ohne
  Neukompilieren justierbar sind.
- **Eigenschafts-Tests** (D-012): Volumen bleibt bis auf Zunder exakt erhalten; Wasser härtet
  stärker als Öl; unter ≈ 700 °C keine Verformung, aber mehr Spannung; einseitiges Strecken
  krümmt.
- **Determinismus-Test:** Skript mit festen Aktionen, Hash des Endzustands als Golden File. Ein
  Vergleich im selben Prozess beweist wenig; der Test muss auf mehreren Plattformen laufen
  (Linux, Windows, ggf. WASM). Dafür braucht es CI, die es im Repo noch nicht gibt.
- **Debug-Werkzeug** im Bevy-Viewer (wie `geodump`): Klinge von oben und von der Seite,
  Temperatur und Härte als Farbverlauf, Skripte abspielen.

## Offen

- Konkrete Formeln (Verformbarkeit, Abkühlrate, Härte/Zähigkeit), entstehen im Prototyp
- Log-Encoding (bei #25)
