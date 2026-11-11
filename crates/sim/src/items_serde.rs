//! Per-item arrays (one slot per `ItemKind`) are longer than serde's built-in array support reaches, so they are
//! written as a sequence and read back into the fixed array. Saves from before flint (format 17) wrote them as
//! fixed 32-slot tuples; while `LEGACY` is set they are read that way, the new slots left empty.

use crate::ItemKind;
use serde::de::{Deserialize, Deserializer, Error, SeqAccess, Visitor};
use serde::ser::{Serialize, Serializer};
use std::cell::Cell;

/// Item slots in format-17 saves.
pub(crate) const LEGACY_COUNT: usize = 32;

thread_local! {
    pub(crate) static LEGACY: Cell<bool> = const { Cell::new(false) };
}

pub fn serialize<T: Serialize, S: Serializer>(a: &[T; ItemKind::COUNT], s: S) -> Result<S::Ok, S::Error> {
    a.as_slice().serialize(s)
}

pub fn deserialize<'de, T: Deserialize<'de> + Default + Copy, D: Deserializer<'de>>(d: D) -> Result<[T; ItemKind::COUNT], D::Error> {
    if LEGACY.with(|l| l.get()) {
        struct Old<T>(std::marker::PhantomData<T>);
        impl<'de, T: Deserialize<'de> + Default + Copy> Visitor<'de> for Old<T> {
            type Value = [T; ItemKind::COUNT];
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(f, "{LEGACY_COUNT} item counts")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut out = [T::default(); ItemKind::COUNT];
                for (i, slot) in out.iter_mut().enumerate().take(LEGACY_COUNT) {
                    *slot = seq.next_element()?.ok_or_else(|| A::Error::invalid_length(i, &self))?;
                }
                Ok(out)
            }
        }
        return d.deserialize_tuple(LEGACY_COUNT, Old(std::marker::PhantomData));
    }
    // items are only ever appended: a save from before newer items has a shorter list, the new slots stay empty
    let v: Vec<T> = Vec::deserialize(d)?;
    if v.len() > ItemKind::COUNT {
        return Err(D::Error::invalid_length(v.len(), &"at most one count per item kind"));
    }
    let mut out = [T::default(); ItemKind::COUNT];
    out[..v.len()].copy_from_slice(&v);
    Ok(out)
}
