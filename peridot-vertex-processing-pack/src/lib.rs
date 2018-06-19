//! Vertex Processing Stage Container

extern crate bedrock;

use bedrock as br;
use std::io::{Write, BufRead, Seek, SeekFrom, Result as IOResult, Error as IOError, ErrorKind, Cursor};
use std::io::BufReader;
use std::fs::File;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PvpContainer {
    pub vertex_bindings: Vec<br::vk::VkVertexInputBindingDescription>,
    pub vertex_attributes: Vec<br::vk::VkVertexInputAttributeDescription>,
    pub vertex_shader: Vec<u8>,
    pub fragment_shader: Option<Vec<u8>>,
}
impl PvpContainer {
    pub fn empty() -> Self {
        PvpContainer {
            vertex_bindings: Vec::new(), vertex_attributes: Vec::new(), vertex_shader: Vec::new(),
            fragment_shader: None
        }
    }

    pub fn write<W: Write>(&self, writer: &mut W) -> IOResult<()> {
        writer.write(b"PVP\x01")?;  // ヘッダ(シグネチャとバージョン)

        // バイナリを裏で構築しつつオフセット値を書き出す
        let mut blob = Cursor::new(Vec::new());
        self.vertex_bindings.binary_serialize(&mut blob)?;
        VariableUInt(blob.seek(SeekFrom::Current(0))? as _).write(writer)?;
        self.vertex_attributes.binary_serialize(&mut blob)?;
        VariableUInt(blob.seek(SeekFrom::Current(0))? as _).write(writer)?;
        self.vertex_shader.binary_serialize(&mut blob)?;
        if let Some(ref b) = self.fragment_shader {
            VariableUInt(blob.seek(SeekFrom::Current(0))? as _).write(writer)?;
            b.binary_serialize(&mut blob)?;
        }
        else { VariableUInt(0).write(writer)?; }

        writer.write(&blob.into_inner()).map(drop)
    }
}

pub struct PvpContainerReader<R: BufRead + Seek> {
    vb_offset: usize, va_offset: usize, vsh_offset: usize, fsh_offset: Option<usize>,
    reader: R
}
impl<R: BufRead + Seek> PvpContainerReader<R> {
    pub fn new(mut reader: R) -> IOResult<Self> {
        let mut signature = [0u8; 4];
        reader.read_exact(&mut signature)?;
        if &signature != b"PVP\x01" {
            return Err(IOError::new(ErrorKind::Other, "Signature mismatch: Invalid or corrupted Peridot Vertex Processing file"));
        }

        let VariableUInt(va_offset) = VariableUInt::read(&mut reader)?;
        let VariableUInt(vsh_offset) = VariableUInt::read(&mut reader)?;
        let VariableUInt(fsh_offset_0) = VariableUInt::read(&mut reader)?;
        let blob_offset = reader.seek(SeekFrom::Current(0))? as usize;

        return Ok(PvpContainerReader {
            vb_offset: blob_offset, va_offset: va_offset + blob_offset,
            vsh_offset: vsh_offset + blob_offset,
            fsh_offset: if fsh_offset_0 == 0 { None } else { Some(fsh_offset_0 + blob_offset) },
            reader
        });
    }

    pub fn read_vertex_bindings(&mut self) -> IOResult<Vec<br::vk::VkVertexInputBindingDescription>> {
        self.reader.seek(SeekFrom::Start(self.vb_offset as _))?;
        Vec::<_>::binary_unserialize(&mut self.reader)
    }
    pub fn read_vertex_attributes(&mut self) -> IOResult<Vec<br::vk::VkVertexInputAttributeDescription>> {
        self.reader.seek(SeekFrom::Start(self.va_offset as _))?;
        Vec::<_>::binary_unserialize(&mut self.reader)
    }
    pub fn read_vertex_shader(&mut self) -> IOResult<Vec<u8>> {
        self.reader.seek(SeekFrom::Start(self.vsh_offset as _))?;
        Vec::<u8>::binary_unserialize(&mut self.reader)
    }
    pub fn is_fragment_stage_provided(&mut self) -> bool { self.fsh_offset.is_some() }
    pub fn read_fragment_shader(&mut self) -> IOResult<Vec<u8>> {
        self.reader.seek(SeekFrom::Start(self.fsh_offset.unwrap() as _))?;
        Vec::<u8>::binary_unserialize(&mut self.reader)
    }

    pub fn into_container(mut self) -> IOResult<PvpContainer> {
        Ok(PvpContainer {
            vertex_bindings: self.read_vertex_bindings()?,
            vertex_attributes: self.read_vertex_attributes()?,
            vertex_shader: self.read_vertex_shader()?,
            fragment_shader: if self.is_fragment_stage_provided() { Some(self.read_fragment_shader()?) } else { None }
        })
    }
}
impl PvpContainerReader<BufReader<File>> {
    pub fn from_file<P: AsRef<Path>>(path: P) -> IOResult<Self> {
        File::open(path).and_then(|fp| Self::new(BufReader::new(fp)))
    }
}

trait BinarySerializeVkStructures {
    fn binary_serialize<W: Write>(&self, sink: &mut W) -> IOResult<()>;
    fn binary_unserialize<R: BufRead>(source: &mut R) -> IOResult<Self> where Self: Sized;
    fn serialize_into_memory(&self) -> IOResult<Vec<u8>> {
        let mut sink = Cursor::new(Vec::new());
        self.binary_serialize(&mut sink).map(|_| sink.into_inner())
    }
}
impl BinarySerializeVkStructures for br::vk::VkVertexInputBindingDescription {
    fn binary_serialize<W: Write>(&self, sink: &mut W) -> IOResult<()> {
        VariableUInt(self.inputRate as _).write(sink)?;
        VariableUInt(self.binding as _).write(sink)?;
        VariableUInt(self.stride as _).write(sink)
    }
    fn binary_unserialize<R: BufRead>(source: &mut R) -> IOResult<Self> where Self: Sized {
        let VariableUInt(input_rate) = VariableUInt::read(source)?;
        let VariableUInt(binding) = VariableUInt::read(source)?;
        let VariableUInt(stride) = VariableUInt::read(source)?;
        return Ok(br::vk::VkVertexInputBindingDescription {
            inputRate: input_rate as _, binding: binding as _, stride: stride as _
        });
    }
}
impl BinarySerializeVkStructures for br::vk::VkVertexInputAttributeDescription {
    fn binary_serialize<W: Write>(&self, sink: &mut W) -> IOResult<()> {
        VariableUInt(self.location as _).write(sink)?;
        VariableUInt(self.binding as _).write(sink)?;
        VariableUInt(self.offset as _).write(sink)?;
        VariableUInt(self.format as _).write(sink)
    }
    fn binary_unserialize<R: BufRead>(source: &mut R) -> IOResult<Self> where Self: Sized {
        let VariableUInt(location) = VariableUInt::read(source)?;
        let VariableUInt(binding) = VariableUInt::read(source)?;
        let VariableUInt(offset) = VariableUInt::read(source)?;
        let VariableUInt(format) = VariableUInt::read(source)?;
        return Ok(br::vk::VkVertexInputAttributeDescription {
            location: location as _, binding: binding as _, offset: offset as _,
            format: format as _
        });
    }
}
impl<T: BinarySerializeVkStructures> BinarySerializeVkStructures for Vec<T> {
    fn binary_serialize<W: Write>(&self, sink: &mut W) -> IOResult<()> {
        VariableUInt(self.len()).write(sink)?;
        for x in self { x.binary_serialize(sink)?; } return Ok(());
    }
    fn binary_unserialize<R: BufRead>(source: &mut R) -> IOResult<Self> where Self: Sized {
        let VariableUInt(element_count) = VariableUInt::read(source)?;
        let mut vs = Vec::with_capacity(element_count);
        for _ in 0 .. element_count { vs.push(T::binary_unserialize(source)?); }
        return Ok(vs);
    }
}
impl BinarySerializeVkStructures for Vec<u8> {
    fn binary_serialize<W: Write>(&self, sink: &mut W) -> IOResult<()> {
        VariableUInt(self.len()).write(sink)?; sink.write(self).map(drop)
    }
    fn binary_unserialize<R: BufRead>(source: &mut R) -> IOResult<Self> where Self: Sized {
        let VariableUInt(element_count) = VariableUInt::read(source)?;
        let mut buf = vec![0u8; element_count];
        source.read_exact(&mut buf).map(|_| buf)
    }
}

/// octet variadic unsigned integer
struct VariableUInt(usize);
impl VariableUInt {
    fn write<W: Write>(&self, writer: &mut W) -> IOResult<()> {
        let (mut n, mut buf) = (self.0 >> 8, vec![(self.0 & 0xff) as u8]);
        while (*buf.last().unwrap() & 0x80) != 0 {
            buf.push((n & 0xff) as _); n >>= 8;
        }
        writer.write(&buf).map(drop)
    }
    fn read<R: BufRead>(reader: &mut R) -> IOResult<Self> {
        let (mut v, mut shifts) = (0usize, 0usize);
        loop {
            let (consumed, done) = {
                let mut available = match reader.fill_buf() {
                    Ok(v) => v,
                    Err(e) => if e.kind() == ErrorKind::Interrupted { continue; } else { return Err(e); }
                };
                let (mut consumed, mut done) = (0, false);
                while !available.is_empty() {
                    v |= (available[0] as usize) << shifts;
                    shifts += 8;
                    consumed += 1;
                    if (available[0] & 0x80) == 0 { done = true; break; }
                    available = &available[1..];
                }
                (consumed, done)
            };
            reader.consume(consumed);
            if done { return Ok(VariableUInt(v)); }
        }
    }
}
