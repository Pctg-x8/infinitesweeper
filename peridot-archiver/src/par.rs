//! Peridot Archive

use peridot_serialization_utils::*;
use std::io::prelude::{Write, BufRead};
use std::io::{Result as IOResult, Error as IOError, ErrorKind};
use std::mem::transmute;
use std::collections::HashMap;
use libflate::deflate as zlib; use lz4; use zstd;
use crc::crc32;

#[repr(C)] pub struct LinearPaired2u64(u64, u64);
#[repr(C)] pub struct AssetEntryHeadingPair { pub byte_length: u64, pub relative_offset: u64 }
impl AssetEntryHeadingPair {
    pub fn write<W: Write>(&self, writer: &mut W) -> IOResult<usize> {
        writer.write(unsafe { &transmute::<_, &[u8; 8 * 2]>(self)[..] }).map(|_| 16)
    }
    pub fn read<R: BufRead>(reader: &mut R) -> IOResult<Self> {
        let mut sink = AssetEntryHeadingPair { byte_length: 0, relative_offset: 0 };
        reader.read_exact(unsafe { &mut transmute::<_, &mut [u8; 8 * 2]>(&mut sink)[..] }).map(|_| sink)
    }
}
pub fn read_asset_entries<R: BufRead>(reader: &mut R) -> IOResult<HashMap<String, AssetEntryHeadingPair>> {
    let VariableUInt(count) = VariableUInt::read(reader)?;
    if count <= 0 { return Ok(HashMap::new()); }
    let mut elements = HashMap::with_capacity(count as _);
    for _ in 0 .. count {
        let heading = AssetEntryHeadingPair::read(reader)?;
        let PascalString(id_ref) = PascalString::read(reader)?;
        elements.insert(id_ref, heading);
    }
    return Ok(elements);
}
/// return -> written bytes(raw)
pub fn write_asset_entries<W: Write>(entries: &HashMap<String, AssetEntryHeadingPair>, writer: &mut W) -> IOResult<usize> {
    let mut written_bytes = VariableUInt(entries.len() as _).write(writer)?;
    for (n, h) in entries { written_bytes += h.write(writer).and_then(|w1| PascalStr(n).write(writer).map(move |w2| w1 + w2))?; }
    return Ok(written_bytes);
}

/// 展開後のサイズが値として入る。圧縮指定時には無視されるので適当な値を指定する
pub enum CompressionMethod {
    None, Zlib(u64), Lz4(u64), Zstd11(u64)
}
pub fn read_file_header<R: BufRead>(reader: &mut R) -> IOResult<(CompressionMethod, u32)> {
    let mut signature = [0u8; 4];
    reader.read_exact(&mut signature[..]).map(drop)?;
    let mut sink_64 = 0u64;
    let comp = match &signature {
        b"par " => CompressionMethod::None,
        b"pard" => reader.read_exact(unsafe { &mut transmute::<_, &mut [u8; 8]>(&mut sink_64)[..] })
            .map(|_| CompressionMethod::Zlib(sink_64))?,
        b"parz" => reader.read_exact(unsafe { &mut transmute::<_, &mut [u8; 8]>(&mut sink_64)[..] })
            .map(|_| CompressionMethod::Lz4(sink_64))?,
        b"par1" => reader.read_exact(unsafe { &mut transmute::<_, &mut [u8; 8]>(&mut sink_64)[..] })
            .map(|_| CompressionMethod::Zstd11(sink_64))?,
        _ => return Err(IOError::new(ErrorKind::Other, "Signature Mismatch or Unsupported Compression method"))
    };
    let mut crc32 = 0u32;
    reader.read_exact(unsafe { &mut transmute::<_, &mut [u8; 4]>(&mut crc32)[..] }).map(drop)?;
    return Ok((comp, crc32));
}

use std::io::Cursor;
pub struct ArchiveWrite(pub CompressionMethod, pub HashMap<String, AssetEntryHeadingPair>, pub Vec<u8>);
impl ArchiveWrite {
    pub fn new(comp: CompressionMethod) -> Self {
        ArchiveWrite(comp, HashMap::new(), Vec::new())
    }
    pub fn write<W: Write>(&self, writer: &mut W) -> IOResult<()> {
        match self.0 {
            CompressionMethod::None => {
                let mut body = Cursor::new(Vec::new());
                write_asset_entries(&self.1, &mut body)?; body.write_all(&self.2[..])?;

                Self::write_common(writer, b"par ", None, &body.into_inner()[..])
            },
            CompressionMethod::Zlib(_) => {
                let mut body = zlib::Encoder::new(Cursor::new(Vec::new()));
                let uncompressed_bytes = write_asset_entries(&self.1, &mut body)
                    .and_then(|wa| body.write_all(&self.2[..]).map(move |_| wa + self.2.len()))? as u64;

                Self::write_common(writer, b"pard", Some(uncompressed_bytes), &body.finish().into_result()?.into_inner()[..])
            }
            CompressionMethod::Lz4(_) => {
                let mut body = lz4::EncoderBuilder::new().build(Cursor::new(Vec::new()))?;
                let uncompressed_bytes = write_asset_entries(&self.1, &mut body)
                    .and_then(|wa| body.write_all(&self.2[..]).map(move |_| wa + self.2.len()))? as u64;
                let (body, r) = body.finish(); r?;

                Self::write_common(writer, b"parz", Some(uncompressed_bytes), &body.into_inner()[..])
            },
            CompressionMethod::Zstd11(_) => {
                let mut body = zstd::Encoder::new(Cursor::new(Vec::new()), 11)?;
                let uncompressed_bytes = write_asset_entries(&self.1, &mut body)
                    .and_then(|wa| body.write_all(&self.2[..]).map(move |_| wa + self.2.len()))? as u64;
                
                Self::write_common(writer, b"par1", Some(uncompressed_bytes), &body.finish()?.into_inner()[..])
            }
        }
    }
    fn write_common<W: Write>(writer: &mut W, signature: &[u8], uncompressed_bytes: Option<u64>, body: &[u8])
            -> IOResult<()> {
        let checksum = crc32::checksum_ieee(body);
        writer.write_all(signature)?;
        if let Some(ub) = uncompressed_bytes { writer.write_all(unsafe { &transmute::<_, &[u8; 8]>(&ub)[..] })?; }
        writer.write_all(unsafe { &transmute::<_, &[u8; 4]>(&checksum)[..] })?;
        writer.write_all(body)
    }
}
