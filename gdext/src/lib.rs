//! terraforge-gdext: bindet `terraforge-core` in Godot ein (#10, D-021).
//!
//! Eine Datei pro Godot-Klasse:
//! - `body`:    `TerraforgeBody`, Bewegung und Kollision aus dem Core (#13)
//! - `world`:   `TerraforgeWorld`, die öffentliche Weltgenerierung (Höhe, Biom, Spawnpunkt)
//! - `terrain`: `TerraforgeTerrain`, Chunk-Streaming um die Kamera (#11)
//! - `server`:  lokaler Server als Kindprozess (#56, keine Godot-Klasse)
//! - `trees`:   Bäume als MultiMesh pro Spalte und Baum-Mesh (#12, keine Godot-Klasse)
//! - `mesh`:    Core-Meshes → Godot-Meshes (keine Godot-Klasse)
//!
//! Der geheime Untergrund ist nicht enthalten (D-005); Höhlen und Erze kommen später vom Server
//! (#56). Build-Anleitung: `client/README.md`.

mod body;
mod mesh;
mod server;
mod terrain;
mod trees;
mod world;

use godot::prelude::*;

// Kein Build des Clients darf den Untergrund enthalten, auch kein Debug-Build: Godot kann auch
// diesen ausliefern ("Export With Debug"). Der eigene Workspace (`Cargo.toml`) verhindert die
// Feature-Vereinigung mit Viewer/Server; diese Prüfung fängt alles andere ab (z. B. jemand
// schaltet das Feature hier ein).
const _: () = assert!(!terraforge_core::UNDERGROUND_COMPILED, "Untergrund im Client-Build (D-005)");

struct Terraforge;

#[gdextension]
unsafe impl ExtensionLibrary for Terraforge {}
