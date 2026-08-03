/// Serde helpers for large arrays that serde_core doesn't auto-implement.
/// serde 1.0.229 internally splits traits into serde_core which only covers [T; 0..=32].
/// These helpers serialize as Vec<f32> on the wire.

/// Helper for `[f32; EMBED_DIM]`.
pub mod f32_128 {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use crate::EMBED_DIM;

    pub fn serialize<S: Serializer>(arr: &[f32; EMBED_DIM], s: S) -> Result<S::Ok, S::Error> {
        arr.as_slice().serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[f32; EMBED_DIM], D::Error> {
        let v: Vec<f32> = Deserialize::deserialize(d)?;
        if v.len() != EMBED_DIM {
            return Err(serde::de::Error::custom(format!(
                "expected exactly {} floats, got {}",
                EMBED_DIM, v.len()
            )));
        }
        let mut arr = [0.0f32; EMBED_DIM];
        arr.copy_from_slice(&v);
        Ok(arr)
    }
}

/// Helper for `Vec<[f32; EMBED_DIM]>`.
pub mod vec_f32_128 {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use crate::EMBED_DIM;

    pub fn serialize<S: Serializer>(v: &Vec<[f32; EMBED_DIM]>, s: S) -> Result<S::Ok, S::Error> {
        let as_vecs: Vec<&[f32]> = v.iter().map(|a| a.as_slice()).collect();
        as_vecs.serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<[f32; EMBED_DIM]>, D::Error> {
        let nested: Vec<Vec<f32>> = Deserialize::deserialize(d)?;
        nested
            .into_iter()
            .enumerate()
            .map(|(i, v)| {
                if v.len() != EMBED_DIM {
                    return Err(serde::de::Error::custom(format!(
                        "centroid[{}]: expected {} floats, got {}",
                        i, EMBED_DIM, v.len()
                    )));
                }
                let mut arr = [0.0f32; EMBED_DIM];
                arr.copy_from_slice(&v);
                Ok(arr)
            })
            .collect()
    }
}
