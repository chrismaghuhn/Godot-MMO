# Design: Kampf

Status: Entwurf (2026-10-26). Entscheidungen dazu: [D-020](../../DECISIONS.md#d-020-charakter-prozedural-first-person-mit-echtem-körper).

Kampf ist **aktiv und körperbetont**, in First-Person, in Richtung Mount & Blade / Mordhau.
Waffen kommen aus der Schmiede-Simulation ([schmieden.md](schmieden.md)), Körper und Bewegung
aus [charakter.md](charakter.md).

## Grundregeln

- **Richtungsbasierte Schläge:** Die Mausbewegung bestimmt die Richtung (oben, seitlich, Stich).
- **Physik aus der Schmiede-Simulation:** Balance und Masse bestimmen Ausholzeit und Wucht.
  Kopflastig = mehr Wucht, langsamer; leicht = schnell, schwach gegen Rüstung.
- **Parieren, Ausweichen, Ausdauer:** Jeder Schwung kostet Kraft, schwere Waffen mehr.
- **Trefferzonen:** Arm, Bein, Rumpf, Kopf wirken unterschiedlich.
- **Abnutzung:** Treffer auf Rüstung hinterlassen Scharten in der Klinge.
- **Reichweite** aus Armlänge plus Waffenlänge.

## Balance der Proportionen

Proportionen wirken im Kampf. Ausgleich:

- Größere Charaktere haben eine größere Trefferfläche
- verbrauchen mehr Ausdauer
- und weichen langsamer aus.
- Die Spanne der Proportionen ist begrenzt.

## Netzwerk

Nahkampf mit Latenz ist schwierig: serverseitige Trefferprüfung mit Lag-Kompensation. Umsetzung
mit Multiplayer (Phase 6, D-010), das Kampf-Modell in `core/` wird aber von Anfang an so gebaut.

## Offen

- **Waffenarten:** Welche gibt es zum Start (Schwert, Axt, Speer, Dolch, …)? Das Schmiede-Modell
  ist allgemein geplant und deckt sie ab, gebaut wird aber zuerst nur das Schwert.
- **Rüstung:** Wie wirken Material, Dicke und Härte gegen Schnitt, Stich und Wucht?
- **Trefferzonen:** Welche Zone hat welche Wirkung (Schaden, Behinderung, Blutung)?
- **Eingabe:** Wie wird die Schlagrichtung mit Maus und Controller eingegeben?
- **Parieren:** Zeitfenster, Richtung, Wirkung auf beide Klingen
