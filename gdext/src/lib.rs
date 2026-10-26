//! terraforge-gdext: bindet `terraforge-core` in Godot ein (#10, D-021).
//!
//! Eine Datei pro Godot-Klasse:
//! - `world`: `TerraforgeWorld`, die öffentliche Weltgenerierung (Höhe, Biom)
//!
//! Der geheime Untergrund ist nicht enthalten (D-005); Höhlen und Erze kommen später vom Server
//! (#56). Build-Anleitung: `client/README.md`.

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
