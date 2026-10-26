//! Der geheime Seed für den Untergrund (D-005).
//!
//! Absichtlich unbequem: kein `Default`, kein `Display`, `Debug` zeigt den Wert nicht,
//! und es gibt keine Ableitung aus dem öffentlichen Seed außer der ausdrücklich
//! unsicheren Entwickler-Variante.

use std::fmt;

/// Geheimer Seed. Der Server erzeugt ihn einmal pro Welt zufällig und speichert ihn;
/// er darf nie an Clients gesendet oder geloggt werden.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SecretSeed(u64);

impl SecretSeed {
    pub fn from_u64(value: u64) -> Self {
        Self(value)
    }

    /// **Nur für Entwicklung** (Viewer, Tests): leitet einen Seed aus dem öffentlichen ab.
    /// Wer den öffentlichen Seed kennt, kennt damit auch diesen. Nie auf einem Server nutzen.
    pub fn insecure_dev_seed(public_seed: u32) -> Self {
        Self(mix(public_seed as u64 ^ 0x5EC2_E7D0_0D5E_ED00))
    }

    /// Deterministischer Hash aus dem geheimen Seed und beliebigen Zahlen (z. B. Provinz-Zelle
    /// plus Zweck). Grundlage für Zufallsentscheidungen wie "hat diese Provinz Gold?".
    pub(super) fn hash(&self, parts: &[i64]) -> u64 {
        parts.iter().fold(mix(self.0 ^ 0xA5A5_5A5A_C3C3_3C3C), |h, &part| mix(h ^ part as u64))
    }

    /// Wie `hash`, aber gleichverteilt in `0.0..1.0`.
    pub(super) fn unit(&self, parts: &[i64]) -> f32 {
        (self.hash(parts) >> 40) as f32 / (1u64 << 24) as f32
    }

    /// Seed für eine Noise-Ebene des Untergrunds. Jede Ebene bekommt ein eigenes `salt`,
    /// damit Ebenen nicht korrelieren.
    pub fn layer(&self, salt: u64) -> i32 {
        (mix(self.0 ^ mix(salt)) >> 32) as i32
    }
}

impl fmt::Debug for SecretSeed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretSeed(***)")
    }
}

/// SplitMix64-Finalizer: verteilt Bits gleichmäßig, damit benachbarte Eingaben
/// völlig verschiedene Ausgaben liefern.
fn mix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_does_not_leak_the_value() {
        let seed = SecretSeed::from_u64(0xDEAD_BEEF_1234_5678);
        let text = format!("{seed:?}");
        assert_eq!(text, "SecretSeed(***)");
        assert!(!text.contains("dead") && !text.contains("DEAD") && !text.contains("3735928559"));
    }

    #[test]
    fn layers_differ_per_salt_and_per_seed() {
        let a = SecretSeed::from_u64(1);
        let b = SecretSeed::from_u64(2);
        assert_ne!(a.layer(1), a.layer(2));
        assert_ne!(a.layer(1), b.layer(1));
        assert_eq!(a.layer(7), SecretSeed::from_u64(1).layer(7));
    }

    #[test]
    fn hash_is_deterministic_and_spread() {
        let seed = SecretSeed::from_u64(5);
        assert_eq!(seed.hash(&[1, 2, 3]), SecretSeed::from_u64(5).hash(&[1, 2, 3]));
        assert_ne!(seed.hash(&[1, 2, 3]), seed.hash(&[1, 2, 4]));
        assert_ne!(seed.hash(&[1, 2]), SecretSeed::from_u64(6).hash(&[1, 2]));
        let values: Vec<f32> = (0..2000).map(|i| seed.unit(&[i, 7])).collect();
        assert!(values.iter().all(|v| (0.0..1.0).contains(v)));
        let mean = values.iter().sum::<f32>() / values.len() as f32;
        assert!((0.45..0.55).contains(&mean), "ungleich verteilt: {mean}");
    }

    #[test]
    fn dev_seed_is_not_the_public_seed() {
        for public in [0u32, 1, 42, 7, u32::MAX] {
            let dev = SecretSeed::insecure_dev_seed(public);
            assert_ne!(dev, SecretSeed::from_u64(public as u64));
        }
    }
}
