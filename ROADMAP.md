# Roadmap

Ziel: Ein Spiel, in dem **alles von Spielern hergestellt wird** und Welt, Items und Charaktere
**prozedural generiert** sind. Langfristig als MMO.

Leitregel für die Reihenfolge: **Erst muss das Spiel allein Spaß machen, dann Multiplayer, dann MMO.**
Jede Phase endet mit etwas Spielbarem oder Testbarem.

Begründungen zu den Entscheidungen stehen in [DECISIONS.md](DECISIONS.md).
Einzelne Aufgaben, Ideen und Bugs werden als [GitHub Issues](https://github.com/chrismaghuhn/Godot-MMO/issues)
verfolgt, gruppiert nach Milestones (= Phasen).

## Architektur

```
core/     terraforge-core   Rust, engine-unabhängig: Weltgenerierung, später Items/Crafting
viewer/   terraforge-viewer Bevy, Debug-Tool für den Core (+ mapdump)
client/   Godot 4           Spiel-Client, bindet den Core über gdext ein
server/   (geplant)         Rust, headless, autoritativ
```

---

## Phase 0: Fundament (erledigt)

- [x] Höhenkarten-Terrain mit Biomen, Vegetation und Flüssen (Bevy)
- [x] 352 Biom-Varianten in 15 Familien (`core/assets/biomes.ron`)
- [x] Asynchrones Chunk-Streaming
- [x] Weniger hügeliges Terrain (D-007)
- [x] Ruckeln behoben: Fenstertitel-Updates entfernt (D-008)
- [x] Monorepo mit Cargo-Workspace, Core ohne Bevy (D-002)
- [x] Godot-Projekt in `client/`

## Phase 1: Volumen-Terrain im Core

Grundlage für Höhlen und freies Graben (D-004). Entwicklung und Test im Bevy-Viewer.

- [x] Dichtefeld + 3D-Chunks (32³), leere Chunks überspringen: #1
- [x] Mesh-Erzeugung mit Surface Nets: #2
- [x] Trennung öffentliche/geheime Generierung (Oberfläche vs. Untergrund, D-005): #3
- [x] Höhlen: Kavernen + Tunnel + Eingänge an der Oberfläche: #4
  - [x] Kleine Höhlenfetzen: #41
  - [x] ShellHeights pro Spalte cachen: #42
- [x] Gestein und Erze (D-013): #5
  - [x] Material-Kanal pro Voxel + materials.ron: #43
  - [x] Gesteinsschichten und Oberflächenmaterial (öffentlich): #44
  - [x] Provinzen, Erz-Vorkommen, Reinheit (geheim): #45
  - [x] Sichtbarkeits-Filter (Erze erst zeigen, wenn sie freiliegen, D-018): #46
  - [x] Viewer: Materialfarben, Röntgenmodus, Statistik: #47
- [x] Graben/Bauen: Änderungen pro Voxel, Chunk neu vermaschen (D-016): #6
- [x] Bäume auf die Volumen-Oberfläche setzen, stabile IDs, Fällen vorbereitet (D-017): #7
- [x] Cave Culling (Sichtbarkeit über Chunk-Verbindungen, D-006, D-018): #8
- [x] Performance-Messung als festes Werkzeug (Frametimes, Chunk-Zeiten): #9

Phase 1 abgeschlossen am 2026-10-26.

## Phase 2: Godot-Client

Plan und Begründung in D-021. Untergrund und Änderungen kommen schon hier von einem lokalen
Server-Prozess, damit der Client nie `underground` enthält (D-005).

- [x] Streaming als engine-unabhängiger Chunk-Manager in den Core, Viewer darauf umstellen: #55
- [ ] gdext-Crate, Godot lädt `terraforge-core`: #10
- [ ] Chunk-Mesh in Godot anzeigen, dann Streaming um die Kamera: #11
  - Fernterrain (D-015), Wasser-Shader, Debug-Overlay (Frametimes, Chunk-Zeiten)
- [ ] Bäume über `MultiMeshInstance3D`: #12
- [ ] Lokaler Server-Prozess (`server/`, `protocol/`): Untergrund-Chunks, Erz-Filter, Cave Culling im Client: #56
- [ ] Laufen, Fliegen, Kollision mit dem Terrain, Bewegung im Core: #13
- [ ] Graben/Bauen im Client, geprüft vom Server: #14

Ab hier ist der Bevy-Viewer nur noch Debug-Tool.

## Phase 3: Ressourcen und Biome mit Inhalt

- [ ] Materialien mit Eigenschaften (Härte, Dichte, Reinheit, …), abgeleitet aus Ort/Gestein: #15
- [ ] Biom-Varianten mit Daten (Bedingungen, Vegetation, Ressourcen), Wahl nach Bedingungen statt zufällig: #16
- [ ] Erste 30-40 Biome vollständig ausbauen (D-009): #17
- [ ] Abbauen + einfaches Inventar: #18
- [ ] Oberflächen-Hinweise auf Erze (vom Server gesteuert): #48
- [x] Große Regionen (4-8 km) für ein BOTW-Gefühl (D-014): #50
- [ ] Speichern von Weltänderungen (Delta auf den Seed): #19

## Phase 4: Crafting

- [ ] Rezepte als Umwandlungen, Ergebnis-Eigenschaften werden aus den Eingaben **berechnet**: #20
- [ ] Werkzeuge mit Wirkung (Abbaugeschwindigkeit, erreichbare Härte): #21
- [ ] Crafting-UI: #22
- [ ] Prozedurale Item-Optik abhängig vom Material: #23
- [ ] Schmiede-Simulation als Prototyp in `core/` (Aktions-Enum, Hammerschlag, Wärme, Remeshing,
  Abschrecken, Golden-File-Test), Design in [docs/design/schmieden-datenmodell.md](docs/design/schmieden-datenmodell.md) (D-019)

**Entscheidungspunkt:** Macht der Kreislauf Sammeln → Craften → Verbessern allein Spaß?
Wenn nicht: hier nachbessern, bevor Netzwerk dazukommt.

## Phase 5: Prozedurale Charaktere (#24)

- [ ] Charakter-Seed → Proportionen, Farben, Merkmale
- [ ] Gemeinsames Grundskelett für Animationen
- [ ] Charakter-Editor (würfeln + anpassen)

## Phase 6: Multiplayer (2-10 Spieler)

- [ ] `server/`: headless Rust-Server, autoritativ, Clients senden nur Eingaben (D-003): #25
  - Vorgezogen in Phase 2 (#56): lokaler Server-Prozess, Protokoll, Untergrund-Streaming
- [ ] Interest Management + Untergrund-Streaming, Erze erst senden, wenn sie freiliegen (D-005): #26
- [ ] Persistenz (Start: SQLite): #27

## Phase 7: Wirtschaft und Welt (#28)

- [ ] Handel zwischen Spielern
- [ ] Regionale Ressourcen, die Handel nötig machen
- [ ] Bauen (Häuser, Werkstätten)
- [ ] Spieltests mit Freunden

## Phase 8: Richtung MMO (#29)

- [ ] Welt auf mehrere Server aufteilen (Zonen)
- [ ] Accounts, Login, Anti-Cheat
- [ ] Hosting, Lasttests

## Ideen (noch nicht eingeplant)

- Besondere Welt-Features: schwebende Inseln, Wasserfälle, Ruinen, Geysire, Treibsand: #30
- Seen als eigene Gewässer: #31
- Höhlen-Biome auf echte Höhlen abbilden: #32
- [x] LOD für weite Sichtweite (D-015): #33

## Bekannte Bugs und Aufräumarbeiten

Aktueller Stand immer in den [offenen Issues](https://github.com/chrismaghuhn/Godot-MMO/issues?q=is%3Aopen+label%3Abug%2Ctech-debt).

- [x] Gebirgsränder blockartig: #36
- [x] 8 doppelte Biom-Namen: #37
- [x] Viewer ruckelt, wenn gleichzeitig Godot offen ist (nicht mehr reproduzierbar): #38

---

## Einschätzung

- Phasen 1-4 sind allein in einigen Monaten machbar. Danach ist klar, ob die Idee trägt.
- Phase 6 ist der größte Sprung in der Schwierigkeit.
- Phase 8 ist für eine Person sehr viel. Ein Spiel mit kleinen Servern (wie Valheim, Vintage Story)
  wäre kein Scheitern.
