//! Peridot Archive

use peridot_serialization_utils::*;

#[repr(C)] pub struct AssetEntry<'f> {
    pub byte_length: u64, pub relative_offset: u64, pub id_ref: &'f str
}

