//! Deterministic ID types using BLAKE3.
//!
//! - `ConfigHash`: structural identity (component types only, no parameter values).
//! - `FullHash`: exact identity (component types + all parameter values).
//! - `DatasetHash`: identifies a specific dataset (symbols + date range + data content).
//! - `RunId`: unique identifier for a single backtest run.
//! - `OrderId`, `SignalEventId`, `OcoGroupId`: sequential counters.

use serde::{Deserialize, Serialize};
use std::fmt;

// ── Sequential ID types ──────────────────────────────────────────────

macro_rules! seq_id {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub struct $name(pub u64);

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({})", stringify!($name), self.0)
            }
        }
    };
}

seq_id!(OrderId);
seq_id!(SignalEventId);
seq_id!(OcoGroupId);

/// Monotonically increasing ID generator.
#[derive(Debug, Default)]
pub struct IdGen {
    next: u64,
}

impl IdGen {
    pub fn next_order_id(&mut self) -> OrderId {
        let id = OrderId(self.next);
        self.next += 1;
        id
    }

    pub fn next_signal_event_id(&mut self) -> SignalEventId {
        let id = SignalEventId(self.next);
        self.next += 1;
        id
    }

    pub fn next_oco_group_id(&mut self) -> OcoGroupId {
        let id = OcoGroupId(self.next);
        self.next += 1;
        id
    }
}

// ── BLAKE3-based hash types ──────────────────────────────────────────

/// 32-byte BLAKE3 hash wrapper with hex display and serde as hex string.
macro_rules! hash_id {
    ($name:ident) => {
        #[derive(Clone, PartialEq, Eq, Hash)]
        pub struct $name(pub [u8; 32]);

        impl $name {
            pub fn from_bytes(data: &[u8]) -> Self {
                Self(*blake3::hash(data).as_bytes())
            }

            pub fn as_hex(&self) -> String {
                self.0.iter().map(|b| format!("{b:02x}")).collect()
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({})", stringify!($name), &self.as_hex()[..16])
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.as_hex())
            }
        }

        impl Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str(&self.as_hex())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let hex = String::deserialize(d)?;
                let bytes: Vec<u8> = (0..hex.len())
                    .step_by(2)
                    .map(|i| u8::from_str_radix(&hex[i..i + 2], 16))
                    .collect::<Result<_, _>>()
                    .map_err(serde::de::Error::custom)?;
                let arr: [u8; 32] = bytes
                    .try_into()
                    .map_err(|_| serde::de::Error::custom("expected 32 bytes"))?;
                Ok(Self(arr))
            }
        }
    };
}

hash_id!(ConfigHash);
hash_id!(FullHash);
hash_id!(DatasetHash);
hash_id!(RunId);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_gen_is_monotonic() {
        let mut gen = IdGen::default();
        let a = gen.next_order_id();
        let b = gen.next_order_id();
        assert!(b.0 > a.0);
    }

    #[test]
    fn blake3_hash_is_deterministic() {
        let h1 = ConfigHash::from_bytes(b"donchian+atr_trail+worst_case+no_filter");
        let h2 = ConfigHash::from_bytes(b"donchian+atr_trail+worst_case+no_filter");
        assert_eq!(h1, h2);
    }

    #[test]
    fn different_input_different_hash() {
        let h1 = ConfigHash::from_bytes(b"donchian+atr_trail");
        let h2 = ConfigHash::from_bytes(b"donchian+chandelier");
        assert_ne!(h1, h2);
    }

    #[test]
    fn hash_serialization_roundtrip() {
        let h = ConfigHash::from_bytes(b"test data");
        let json = serde_json::to_string(&h).unwrap();
        let deser: ConfigHash = serde_json::from_str(&json).unwrap();
        assert_eq!(h, deser);
    }

    #[test]
    fn hash_hex_is_64_chars() {
        let h = RunId::from_bytes(b"run-1");
        assert_eq!(h.as_hex().len(), 64);
    }
}
