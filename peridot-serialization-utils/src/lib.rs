use std::io::prelude::{BufRead, Write};
use std::io::{Result as IOResult, ErrorKind};

/// octet variadic unsigned integer
struct VariableUInt(u32);
impl VariableUInt {
    fn write<W: Write>(&self, writer: &mut W) -> IOResult<()> {
        Self::iter_fragment(self.0, |v| writer.write(&[v]).map(drop))
    }
    /// u32 to break apart into bytes, and calls closure with fragment
    fn iter_fragment<CB: FnMut(u8) -> IOResult<()>>(v: u32, mut callback: CB) -> IOResult<()> {
        let (n7, nr) = ((v & 0x7f) as u8, v >> 7);
        callback(n7 | if nr != 0 { 0x80 } else { 0 })?;
        if nr != 0 { Self::iter_fragment(nr, callback) } else { Ok(()) }
    }
    fn read<R: BufRead>(reader: &mut R) -> IOResult<Self> {
        let (mut v, mut shifts) = (0u32, 0usize);
        loop {
            let (consumed, done) = {
                let mut available = match reader.fill_buf() {
                    Ok(v) => v,
                    Err(e) => if e.kind() == ErrorKind::Interrupted { continue; } else { return Err(e); }
                };
                let (mut consumed, mut done) = (0, false);
                while !available.is_empty() {
                    v |= ((available[0] & 0x7f) as u32) << shifts;
                    shifts += 7;
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
