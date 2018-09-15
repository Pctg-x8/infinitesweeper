//! Peridot Archive

use peridot_serialization_utils::*;
use std::io::prelude::{Write, BufRead};
use std::io::{Result as IOResult, Error as IOError, ErrorKind};
use std::mem::transmute;
use std::collections::HashMap;

#[repr(C)] pub struct LinearPaired2u64(u64, u64);
#[repr(C)] pub struct AssetEntryHeadingPair { pub byte_length: u64, pub relative_offset: u64 }
impl AssetEntryHeadingPair {
    pub fn write<W: Write>(&self, writer: &mut W) -> IOResult<()> {
        writer.write(unsafe { &transmute::<_, &[u8; 8 * 2]>(self)[..] }).map(drop)
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
pub fn write_asset_entries<W: Write>(entries: &HashMap<String, AssetEntryHeadingPair>, writer: &mut W) -> IOResult<()> {
    VariableUInt(entries.len() as _).write(writer)?;
    for (n, h) in entries { h.write(writer).and_then(|_| PascalStr(n).write(writer))?; }
    return Ok(());
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
    reader.read_exact(unsafe { &mut transmute::<_, &mut [u8; 4]>(&mut sink_64)[..] }).map(drop)?;
    return Ok((comp, crc32));
}

use std::io::Cursor;
pub struct ArchiveWrite(pub CompressionMethod, pub HashMap<String, AssetEntryHeadingPair>, pub Vec<u8>);
impl ArchiveWrite {
    pub fn new(comp: CompressionMethod) -> Self {
        ArchiveWrite(comp, HashMap::new(), Vec::new())
    }
    pub fn write<W: Write>(&self, writer: &mut W) -> IOResult<()> {
        let mut body = Cursor::new(Vec::new());
        write_asset_entries(&self.1, &mut body)?; body.write(&self.2[..]).map(drop)?;
        let body = body.into_inner();
        match self.0 {
            CompressionMethod::None => {
                let signature = b"par ";
                let checksum = 0u32;   // todo
                writer.write(&signature[..]).map(drop)?;
                writer.write(unsafe { &transmute::<_, &[u8; 4]>(&checksum)[..] }).map(drop)?;
                writer.write(&body[..]).map(drop)?;
            },
            CompressionMethod::Zlib(_) => unimplemented!("pard"),//b"pard",
            CompressionMethod::Lz4(_) => unimplemented!("parz"),//b"parz",
            CompressionMethod::Zstd11(_) => unimplemented!("par1"),//b"par1"
        }
        return Ok(());
    }
}
