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
  - [ ] Kleine Höhlenfetzen: #41
  - [ ] ShellHeights pro Spalte cachen: #42
- [ ] Gestein und Erze (D-013): #5
  - [x] Material-Kanal pro Voxel + materials.ron: #43
  - [ ] Gesteinsschichten und Oberflächenmaterial (öffentlich): #44
  - [ ] Provinzen, Erz-Vorkommen, Reinheit (geheim): #45
  - [ ] Sichtbarkeits-Filter (Erze erst zeigen, wenn sie freiliegen): #46
  - [ ] Viewer: Materialfarben, Röntgenmodus, Statistik: #47
- [ ] Graben/Bauen: Änderungen pro Chunk speichern, Chunk neu vermaschen: #6
- [ ] Bäume auf die Volumen-Oberfläche setzen: #7
- [ ] Cave Culling (Sichtbarkeit über Chunk-Verbindungen, D-006): #8
- [ ] Performance-Messung als festes Werkzeug (Frametimes, Chunk-Zeiten): #9

## Phase 2: Godot-Client

- [ ] gdext-Crate, Godot lädt `terraforge-core`: #10
- [ ] Chunk-Mesh in Godot anzeigen, dann Streaming um die Kamera: #11
- [ ] Bäume über `MultiMeshInstance3D`: #12
- [ ] Laufen, Fliegen, Kollision mit dem Terrain: #13
- [ ] Graben/Bauen im Client: #14

Ab hier ist der Bevy-Viewer nur noch Debug-Tool.

## Phase 3: Ressourcen und Biome mit Inhalt

- [ ] Materialien mit Eigenschaften (Härte, Dichte, Reinheit, …), abgeleitet aus Ort/Gestein: #15
- [ ] Biom-Varianten mit Daten (Bedingungen, Vegetation, Ressourcen), Wahl nach Bedingungen statt zufällig: #16
- [ ] Erste 30-40 Biome vollständig ausbauen (D-009): #17
- [ ] Abbauen + einfaches Inventar: #18
- [ ] Oberflächen-Hinweise auf Erze (vom Server gesteuert): #48
- [ ] Speichern von Weltänderungen (Delta auf den Seed): #19

## Phase 4: Crafting

- [ ] Rezepte als Umwandlungen, Ergebnis-Eigenschaften werden aus den Eingaben **berechnet**: #20
- [ ] Werkzeuge mit Wirkung (Abbaugeschwindigkeit, erreichbare Härte): #21
- [ ] Crafting-UI: #22
- [ ] Prozedurale Item-Optik abhängig vom Material: #23

**Entscheidungspunkt:** Macht der Kreislauf Sammeln → Craften → Verbessern allein Spaß?
Wenn nicht: hier nachbessern, bevor Netzwerk dazukommt.

## Phase 5: Prozedurale Charaktere (#24)

- [ ] Charakter-Seed → Proportionen, Farben, Merkmale
- [ ] Gemeinsames Grundskelett für Animationen
- [ ] Charakter-Editor (würfeln + anpassen)

## Phase 6: Multiplayer (2-10 Spieler)

- [ ] `server/`: headless Rust-Server, autoritativ, Clients senden nur Eingaben (D-003): #25
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
- LOD für weite Sichtweite: #33

## Bekannte Bugs und Aufräumarbeiten

Aktueller Stand immer in den [offenen Issues](https://github.com/chrismaghuhn/Godot-MMO/issues?q=is%3Aopen+label%3Abug%2Ctech-debt).

- Viewer-Screenshot speichert nur 1×1 Pixel: #34
- Flussufer sehr steil: #35
- Gebirgsränder blockartig: #36
- 8 doppelte Biom-Namen: #37
- Viewer ruckelt, wenn gleichzeitig Godot offen ist: #38
- Umlaute fehlen im Viewer-Overlay: #39
- Palmen an verschneiten Küsten: #40

---

## Einschätzung

- Phasen 1-4 sind allein in einigen Monaten machbar. Danach ist klar, ob die Idee trägt.
- Phase 6 ist der größte Sprung in der Schwierigkeit.
- Phase 8 ist für eine Person sehr viel. Ein Spiel mit kleinen Servern (wie Valheim, Vintage Story)
  wäre kein Scheitern.
