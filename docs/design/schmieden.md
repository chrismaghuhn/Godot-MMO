# Design: Schmieden

Status: Entwurf (2026-10-26). Entscheidungen dazu: [D-015](../../DECISIONS.md#d-015-schmieden-als-simulation-statt-rezept).

Ziel: Schmieden ist ein **tiefer** Skill statt eines flachen Rezept-Systems. Jedes Schwert ist
einzigartig und sieht so aus, wie es geschmiedet wurde. Gehört zu Phase 4 (Crafting, #20, #23).

## Grundidee

Eine Klinge ist ein **Datenmodell**, das sich mit jeder Aktion verändert. Aussehen und Werte
werden daraus **berechnet**, nicht ausgewürfelt. Es gibt keinen "Legendär"-Wurf: Ein Meisterwerk
ist ein Schwert aus gutem Material, ohne Fehler und mit stimmiger Form.

### Segmente

Die Klinge besteht aus 32-64 Segmenten entlang der Länge. Pro Segment (Entwurf, wird noch
festgelegt):

- Breite, Dicke, Querschnitt (Raute, Linse, Hohlkehle, …), Fasen
- Lage der Mittellinie (Krümmung, Verzug)
- Kohlenstoffgehalt (hart/spröde ↔ weich/zäh)
- Gefüge bzw. Korngröße
- Schichten (Anzahl, Verzerrung) für das Damastmuster
- Härte nach dem Abschrecken, Lehmauftrag, Anlasstemperatur
- Fehler als Schweregrad 0.0-1.0 (siehe unten)
- Temperatur (nur während der Arbeit)

## Ablauf

| Schritt | Spieler tut | Ergebnis im Datenmodell |
|---|---|---|
| Material | Barren wählen, ggf. Stähle kombinieren | Kohlenstoff, Verunreinigungen, Herkunft (Provinz, D-013) |
| Erhitzen | Glutfarbe lesen, Esse regeln | Temperatur; zu heiß: Korn wächst, Entkohlung; zu kalt: Risse |
| Schmieden | Ort, Stärke, Hammer wählen | Form der Klinge |
| Falten | Anzahl, Stähle | Damastmuster, weniger Schlacke, etwas Materialverlust |
| Härten | Lehm auftragen, Wasser/Öl | Härtelinie (Hamon) dort, wo der Lehm war; Verzugs-/Rissrisiko |
| Anlassen | Anlassfarben beobachten | Balance Härte ↔ Zähigkeit |
| Schleifen, Griff | Fasenwinkel, Griffmaterial | Schärfe, Haltbarkeit, Balancepunkt |

Werte folgen aus den Daten: Balance aus der Massenverteilung, Schnitthaltigkeit aus der Härte an
der Schneide, Bruchfestigkeit aus der Zähigkeit im Rücken und den Fehlern.

## Fortschritt: Wissen und Wahrnehmung

Der Charakter wird nicht durch Prozentboni besser, sondern durch das, was er **wahrnimmt**:

- Anfänger sieht "rot-orange", Meister sieht "≈ 850 °C, richtig zum Härten".
- Erfahrene bekommen Warnungen vor Spannungen und Rissen.
- Wer ein Erz oft verarbeitet, lernt seine Eigenheiten.

Dazu kommt das Können des Spielers (Farben lesen, Timing, Planung).

## Dauer wächst mit dem Material

Einfaches Material ist schnell, gutes dauert länger. Die Dauer entsteht aus den Eigenschaften
des Materials, nicht aus Timern oder mehr Klicks:

| Material | Richtwert | Was es aufwendiger macht |
|---|---|---|
| Eisen / Weichstahl | 2-5 Min. | Großes Temperaturfenster, kaum Risse, kein Härten |
| Kohlenstoffstahl | 5-10 Min. | Härten nötig, Verzug und Risse möglich |
| Raffinierter Stahl | 10-20 Min. | Falten nötig, um Schlacke zu entfernen |
| Damast / Verbundstahl | 20-40 Min. | Feuerschweißen in engem Fenster, Muster planen |
| Seltene Erze / Meisterwerke | 45+ Min. | Sehr enges Fenster, kühlt schnell aus, Lehmhärtung, langes Anlassen |

Mechanik: engeres Temperaturfenster, weniger Verformung pro Schlag bei hartem Stahl, zusätzliche
Schritte, und Fehler werden mit wertvollerem Material teurer.

Damit es nicht nervt:

- **Routine:** Gemeisterte Schritte bei Material unter dem eigenen Können lassen sich
  zusammenfassen. Material an der Grenze des Könnens muss Schritt für Schritt bearbeitet werden.
- **Pausieren:** Das Werkstück ist ein Item, kann liegen bleiben und weitergegeben werden.
- Die Zeit fließt in die kritischen Momente (Schweißhitze, Abschrecken), nicht ins Ausschmieden.

## Fehler: schlechter, nicht kaputt

Vorerst misslingt nichts komplett. Fehler sind sichtbar, dauerhaft und nur teilweise reparierbar:

| Fehler | Ursache | Sichtbar als | Reparierbar? |
|---|---|---|---|
| Grobes Korn | Zu heiß | Matter, körniger Schliff | Ja, Normalisieren (kostet Zeit) |
| Entkohlung | Zu lange in der Glut | Weiche Schneide, schwacher Hamon | Nein, nur abschleifen (Klinge wird schmaler) |
| Haarriss | Zu kalt geschlagen, hartes Abschrecken | Feine Linie | Teilweise, bleibt ein Schwachpunkt |
| Verzug | Ungleichmäßiges Abschrecken | Krumme Klinge | Ja, Richten, mit Rissrisiko |
| Schlacke-Einschluss | Zu wenig gefaltet | Dunkle Flecken im Muster | Nein |

Fehler werden als Schweregrad gespeichert, damit später eine Bruch-Schwelle möglich ist
(z. B. "ab 0.9 bricht die Klinge"), ohne das Datenmodell zu ändern.

## Gemeinsam schmieden

Mehrere Spieler können an einem Werkstück arbeiten. Umsetzung erst mit Multiplayer (Phase 6,
D-010), das Datenmodell berücksichtigt es aber von Anfang an.

Rollen:

- **Meister:** hält und dreht das Werkstück, tippt mit dem kleinen Hammer Ziel und Stärke an
  und gibt den Takt vor.
- **Zuschläger:** schwerer Hammer, trifft im Takt. Gutes Timing = volle Wirkung, schlechtes =
  schiefer Schlag.
- **Heizer** (optional): Blasebalg, hält die Temperatur stabil, das Fenster wird größer.

Vorteile: mehr Verformung pro Hitze (schneller, weniger Fehlerrisiko); große Stücke
(Zweihänder, Rüstungsplatten) allein nur sehr langsam. Allein bleibt immer möglich.

Lernen: Ein Zuschläger sieht während der Arbeit die **Wahrnehmung des Meisters** und lernt
dadurch schneller (Lehrling-Meister-Beziehung).

Signatur: Das Schmiedezeichen gehört dem Meister, die Mitwirkenden stehen in der Geschichte
des Schwerts.

Netzwerk: Der Server gibt den Takt vor, Schläge werden relativ zum Takt bewertet, mit
tolerantem Fenster (≈ 150 ms). Schläge sind dieselben Befehle wie beim Solo-Schmieden, nur die
Wirkung hängt von Werkzeug und Timing ab.

## Alles prozedural, keine Assets

Die Klinge braucht keine Modelle oder Texturen: Das Schwert **ist** seine Daten.

- **`core/` (Rust):** Simulation (Schläge, Hitze, Abschrecken) und Werte. Deterministisch, gleich
  auf Server und Client (D-001, D-003).
- **Godot:** baut nur Mesh und Shader-Parameter aus den Segmentdaten. Erfindet nichts dazu,
  damit Aussehen und Spielwerte nie auseinanderlaufen.

| Teil | Erzeugung |
|---|---|
| Klingenform | Querschnitt pro Segment, entlang der (gekrümmten) Mittellinie verbunden (Loft) |
| Damastmuster | Shader: Schichtfunktion mit Verzerrung aus Noise, Schlägen und Verdrehungen; Seed = Schwert-ID |
| Hamon | Lehmauftrag pro Segment → Linie und Glanzunterschied |
| Anlassfarben | Anlasstemperatur → Farbverlauf (stroh, braun, violett, blau) |
| Glut | Temperatur → Schwarzkörperfarbe als Emission (gleichzeitig Gameplay) |
| Hammerspuren | Gespeicherte Schlagpositionen → prozedurale Dellen in der Normal-Map |
| Griff, Parierstange, Knauf | Parametrisch (Lathe/Extrude), Wicklung als Shader-Muster |
| Abnutzung | Scharten, Rost als Werte pro Segment |

Übertragen und gespeichert wird nur das Datenmodell (wenige hundert Bytes), nicht das Mesh.
Aus der Ferne reicht eine einfache Form mit Grundfarbe (LOD), Muster-Shader nur in der Nähe.
Stil eher klar und leicht stilisiert (deutliche Fasen, kräftige Muster), damit es nicht
"technisch" wirkt.

## Welt und Wirtschaft

- Erze sind regional (D-013, D-014): Eisen aus verschiedenen Provinzen unterscheidet sich in
  Reinheit und Spurenelementen. Das schafft Handel und lokale Schmiedetraditionen.
- Jedes Schwert hat eine Geschichte: Schmied, Mitwirkende, Herkunft des Erzes, später
  Reparaturen, Vorbesitzer.
- Klingen nutzen sich ab und können nachgeschliffen oder umgeschmiedet werden.
- Gute Schwerter kosten echte Zeit und Können und bleiben dadurch wertvoll, ohne künstliche
  Seltenheit.
- Anti-Bot: Farben lesen und schwankendes Material machen stures Wiederholen per Makro wertlos.

## Offen

- Genaues Datenmodell eines Segments und was ein Hammerschlag damit macht
- Wie aktiv das Schmieden ist (Echtzeit-Schläge, geplant oder gemischt)
- Nur Schwerter oder ein allgemeines Modell für alle Waffen und Rüstungen
- Wie realistisch die Metallurgie ist, ob es magische Erze gibt
