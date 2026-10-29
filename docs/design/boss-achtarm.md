# Design: Achtarm-Boss

Status: Idee (2026-10-29), noch nicht eingeplant (#61). Setzt das Kampf-Modell voraus ([kampf.md](kampf.md)).

Ein Boss, der auf **zwei Beinen** läuft und **acht Arme** hat. Die vier Armpaare schlagen im Takt
der Musik zu, jedes Paar in einem anderen Notenwert. Gesteuert wird der Boss nicht von einem
Skript, sondern von einem **trainierten neuronalen Netz**.

## Armpaare und Rhythmus

| Armpaar | Notenwert | Schläge pro Takt | Charakter                  |
|---------|-----------|------------------|----------------------------|
| 1 (oben)  | Ganze   | 1                | langsam, schwer, viel Wucht |
| 2       | Halbe     | 2                |                            |
| 3       | Viertel   | 4                |                            |
| 4 (unten) | Achtel  | 8                | schnell, leicht, Stiche    |

- Auf der **Eins** fallen alle Notenwerte zusammen: Hier sind große Kombinationen möglich.
- Der Spieler kann den Rhythmus hören und lernen. Der Kampf wird zu einem **Rhythmus-Parier-Spiel**,
  passend zum richtungsbasierten Kampf mit Parieren aus [kampf.md](kampf.md).

## Aufteilung: was fest ist, was das Netz entscheidet

| Teil            | Lösung                                                                   |
|-----------------|--------------------------------------------------------------------------|
| Beine / Laufen  | Normale Bewegung wie beim Spieler, Animation im Code mit Fuß-IK ([charakter.md](charakter.md)). Das Netz gibt nur die Richtung vor. |
| Arme            | IK-Ziele pro Arm, Schlagrichtungen wie im Kampf-Modell                   |
| Timing          | **Fest:** Ein Armpaar darf nur auf seinem Notenwert zuschlagen. Das Netz merkt einen Schlag vor, ausgelöst wird er auf dem Beat. |
| Entscheidungen  | **Netz:** wohin laufen, welches Armpaar schlägt, aus welcher Richtung, wann eine Kombination |

Begründung:

- Zweibeiniges Laufen per Reinforcement Learning zu lernen ist sehr aufwendig und sieht oft
  unnatürlich aus. Das Netz trifft deshalb nur taktische Entscheidungen.
- Das Timing wird hart erzwungen, damit der Rhythmus immer sauber bleibt, egal was das Netz lernt.

## Netz

- **Eingaben:** Position und Geschwindigkeit des Spielers, Abstand, Ausrichtung, eigene HP und
  Ausdauer, Zustand des Spielers (holt aus, pariert, …), **Beat-Phase pro Notenwert** (sin/cos).
- **Ausgaben:** Bewegungsrichtung, pro Armpaar: angreifen ja/nein und Schlagrichtung.
- **Belohnung:** Treffer am Spieler, Bonus für gleichzeitige Treffer mehrerer Armpaare,
  Bonus für vorher erkennbare Angriffe (Fairness), Strafe für eigenen Schaden.

## Training und Ausführung

- Der Server ist autoritativ (D-003), das Netz läuft deshalb **auf dem Server**, nicht in Godot.
  Ausführung als ONNX-Modell in Rust (z. B. `tract` oder `ort`).
- Kampf-Logik und Beat-Uhr liegen in `core/` (engine-unabhängig). Damit lässt sich der Kampf
  **headless und schnell** simulieren und als Trainingsumgebung nutzen (z. B. PPO in Python über
  eine Anbindung an `core/`).
- Gegner im Training: zuerst ein einfacher geskripteter Spieler-Bot, später Self-Play.
- Schwierigkeitsstufen bzw. Boss-Phasen aus verschiedenen Trainingsständen (Phase 2 = länger
  trainiertes Netz).

## Reihenfolge

1. Kampf-Grundlagen in `core/`: HP, Treffer, Schlagrichtung, Parieren ([kampf.md](kampf.md))
2. Beat-Uhr in `core/`, deterministisch, vom Server vorgegeben
3. Boss mit Zufalls- oder Regel-KI: Fühlt sich der Kampf gut an?
4. Netz trainieren, das die Regel-KI ersetzt

## Risiken

- **Zu stark oder unfair:** Ein trainiertes Netz findet oft optimale, aber unspaßige Strategien.
  Gegenmittel: erkennbare Angriffe belohnen, Reaktionszeit begrenzen, schwächere Trainingsstände
  verwenden.
- **Schwer zu debuggen:** Verhalten ist nicht direkt im Code lesbar. Die Regel-KI aus Schritt 3
  bleibt als Vergleich und Rückfallebene erhalten.
- **Aufwand:** Training, Werkzeuge und Balancing sind ein eigenes Projekt neben dem Spiel.

## Offen

- Musik: feste Stücke pro Boss oder prozedural erzeugt? Wechselt das Tempo im Kampf?
- Was passiert, wenn der Spieler einen Arm trifft oder abtrennt (Trefferzonen)?
- Ist es ein einzelner Boss oder eine ganze Gegnerart mit verschiedenen Armzahlen und Takten?
- Wie wird der Boss im Multiplayer mit mehreren Spielern gesteuert (Eingaben pro Spieler, Ziel wählen)?
