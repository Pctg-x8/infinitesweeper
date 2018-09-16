
extern crate peridot_serialization_utils;
extern crate clap; extern crate glob; mod par; extern crate libc;
extern crate crc; extern crate lz4; extern crate libflate; extern crate zstd;
use clap::{App, Arg, ArgMatches};
use std::fs::{metadata, read_dir, read, File};
use std::io::{BufReader, Cursor, SeekFrom};
use std::io::prelude::{Read, Write, BufRead, Seek};
use std::io::Result as IOResult;
use std::collections::HashMap;

pub enum WhereArchive { OnMemory(Vec<u8>), FromIO(BufReader<File>) }
impl WhereArchive {
    pub fn on_memory(&mut self) -> IOResult<&[u8]> {
        let replace_buf = if let WhereArchive::FromIO(ref mut r) = self {
            let mut buf = Vec::new();
            r.read_to_end(&mut buf)?; Some(buf)
        }
        else { None };
        if let Some(b) = replace_buf { std::mem::replace(self, WhereArchive::OnMemory(b)); }
        match self {
            WhereArchive::OnMemory(ref b) => Ok(b), _ => unreachable!()
        }
    }
}
pub enum EitherArchiveReader { OnMemory(Cursor<Vec<u8>>), FromIO(BufReader<File>) }
impl EitherArchiveReader {
    pub fn new(a: WhereArchive) -> Self {
        match a {
            WhereArchive::FromIO(r) => EitherArchiveReader::FromIO(r),
            WhereArchive::OnMemory(b) => EitherArchiveReader::OnMemory(Cursor::new(b))
        }
    }
    pub fn unwrap(self) -> WhereArchive {
        match self {
            EitherArchiveReader::FromIO(r) => WhereArchive::FromIO(r),
            EitherArchiveReader::OnMemory(c) => WhereArchive::OnMemory(c.into_inner())
        }
    }
}
impl Read for EitherArchiveReader {
    fn read(&mut self, buf: &mut [u8]) -> IOResult<usize> {
        match self {
            EitherArchiveReader::FromIO(ref mut r) => r.read(buf),
            EitherArchiveReader::OnMemory(ref mut c) => c.read(buf)
        }
    }
}
impl BufRead for EitherArchiveReader {
    fn fill_buf(&mut self) -> IOResult<&[u8]> {
        match self {
            EitherArchiveReader::FromIO(ref mut r) => r.fill_buf(),
            EitherArchiveReader::OnMemory(ref mut c) => c.fill_buf()
        }
    }
    fn consume(&mut self, amt: usize) {
        match self {
            EitherArchiveReader::FromIO(ref mut r) => r.consume(amt),
            EitherArchiveReader::OnMemory(ref mut c) => c.consume(amt)
        }
    }
}
impl Seek for EitherArchiveReader {
    fn seek(&mut self, pos: SeekFrom) -> IOResult<u64> {
        match self {
            EitherArchiveReader::FromIO(ref mut r) => r.seek(pos),
            EitherArchiveReader::OnMemory(ref mut c) => c.seek(pos)
        }
    }
}

pub struct ArchiveRead {
    entries: HashMap<String, par::AssetEntryHeadingPair>, content: EitherArchiveReader,
    content_baseptr: u64
}
impl ArchiveRead {
    pub fn from_file<P: AsRef<Path>>(path: P, check_integrity: bool) -> IOResult<Self> {
        let mut fi = File::open(path).map(BufReader::new).unwrap();
        let (comp, crc) = par::read_file_header(&mut fi).unwrap();
        // println!("Compression Method: {:?}", comp);
        // println!("Checksum: 0x{:08x}", crc);
        let mut body = WhereArchive::FromIO(fi);
        if check_integrity {
            // std::io::stdout().write_all(b"Checking archive integrity...").unwrap();
            // std::io::stdout().flush().unwrap();
            let input_crc = crc::crc32::checksum_ieee(&body.on_memory().unwrap()[..]);
            if input_crc != crc {
                panic!("Checking Integrity Failed: Mismatching CRC-32: input=0x{:08x}", input_crc);
            }
            // println!(" ok");
        }
        match comp {
            par::CompressionMethod::Lz4(ub) => {
                let mut sink = Vec::with_capacity(ub as _);
                let mut decoder = lz4::Decoder::new(EitherArchiveReader::new(body)).unwrap();
                decoder.read_to_end(&mut sink).unwrap();
                body = WhereArchive::OnMemory(sink);
            },
            par::CompressionMethod::Zlib(ub) => {
                let mut sink = Vec::with_capacity(ub as _);
                let mut reader = EitherArchiveReader::new(body);
                let mut decoder = libflate::deflate::Decoder::new(reader);
                decoder.read_to_end(&mut sink).unwrap();
                body = WhereArchive::OnMemory(sink);
            },
            par::CompressionMethod::Zstd11(ub) => {
                let mut sink = Vec::with_capacity(ub as _);
                let mut decoder = zstd::Decoder::new(EitherArchiveReader::new(body)).unwrap();
                decoder.read_to_end(&mut sink).unwrap();
                body = WhereArchive::OnMemory(sink);
            },
            _ => ()
        }
        let mut areader = EitherArchiveReader::new(body);
        let entries = par::read_asset_entries(&mut areader).unwrap();
        /*for (n, d) in &entries {
            println!("- {}: {} {}", n, d.relative_offset, d.byte_length);
        }*/
        let content_baseptr = areader.seek(SeekFrom::Current(0)).unwrap();

        return Ok(ArchiveRead {
            entries, content: areader, content_baseptr
        });
    }

    pub fn read_bin(&mut self, path: &str) -> IOResult<Option<Vec<u8>>> {
        if let Some(entry_pair) = self.entries.get(path) {
            self.content.seek(SeekFrom::Start(self.content_baseptr + entry_pair.relative_offset))?;
            let mut sink = Vec::with_capacity(entry_pair.byte_length as _);
            unsafe { sink.set_len(entry_pair.byte_length as _); }
            self.content.read_exact(&mut sink)?;
            return Ok(Some(sink));
        }
        else { return Ok(None); }
    }
}

fn extract(args: &ArgMatches) {
    let mut archive = ArchiveRead::from_file(args.value_of("arc").unwrap(), args.is_present("check")).unwrap();

    if let Some(apath) = args.value_of("apath") {
        if let Some(b) = archive.read_bin(apath).unwrap() {
            let foptr = unsafe { libc::fdopen(libc::dup(1), "wb\x00".as_ptr() as *const _) };
            NativeOfstream::from_stream_ptr(foptr).unwrap().write_all(&b[..]).unwrap();
        }
        else {
            panic!("Entry not found in archive: {}", apath);
        }
    }
}
fn list(args: &ArgMatches) {
    let archive = ArchiveRead::from_file(args.value_of("arc").unwrap(), args.is_present("check")).unwrap();

    for (n, d) in &archive.entries {
        println!("{}: {} {}", n, d.relative_offset, d.byte_length);
    }
}
fn main() {
    let extract_matcher = App::new("extract").version("0.1").author("S.Percentage <Syn.Tri.Naga@gmail.com>")
        .arg(Arg::with_name("arc").value_name("FILE").required(true).help("Archive file"))
        .arg(Arg::with_name("apath").value_name("ASSET_PATH").help("An Asset Path (Optional)"))
        .arg(Arg::with_name("check").long("check-integrity").help("Checks an archive integrity by checksum"));
    let ls_matcher = App::new("list").version("0.1").author("S.Percentage <Syn.Tri.Naga@gmail.com>")
        .arg(Arg::with_name("arc").value_name("FILE").required(true).help("Archive file"))
        .arg(Arg::with_name("check").long("check-integrity").help("Checks an archive integrity by checksum"));
    let create_matcher = App::new("new").version("0.1").author("S.Percentage <Syn.Tri.Naga@gmail.com>")
        .arg(Arg::with_name("ofile").short("o").long("output").value_name("FILE").help("Describes where archive file will be written"))
        .arg(Arg::with_name("ifiled").help("Input File/Directory").required(true).multiple(true))
        .arg(Arg::with_name("cmethod").short("c").long("compress").value_name("METHOD")
            .possible_values(&["lz4", "zlib", "zstd11"]).takes_value(true).help("Describes the compression method"));
    let matcher = App::new("peridot-archive").version("0.1").author("S.Percentage <Syn.Tri.Naga@gmail.com>")
        .subcommands(vec![extract_matcher, create_matcher, ls_matcher]);
    let matches = matcher.get_matches();

    if let Some(matches) = matches.subcommand_matches("new") { new(matches); }
    if let Some(matches) = matches.subcommand_matches("list") { list(matches); }
    if let Some(matches) = matches.subcommand_matches("extract") { extract(matches); }
}

fn new(args: &ArgMatches) {
    let directory_walker = args.values_of("ifiled").unwrap()
        .flat_map(|f| if cfg!(windows) && f.contains('*') {
            Box::new(glob::glob(f).unwrap().flat_map(|f| extract_directory(&f.unwrap())))
        }
        else { extract_directory(Path::new(f)) });
    let compression_method = args.value_of("cmethod").map(|s| match s {
        "lz4" => par::CompressionMethod::Lz4(0),
        "zlib" => par::CompressionMethod::Zlib(0),
        "zstd11" => par::CompressionMethod::Zstd11(0),
        _ => unreachable!()
    }).unwrap_or(par::CompressionMethod::None);
    let mut archive = par::ArchiveWrite::new(compression_method);
    for f in directory_walker {
        // println!("input <<= {}", f.display());
        let fstr = f.to_str().unwrap();
        if archive.1.contains_key(fstr) {
            eprintln!("Warn: {:?} has already been added", fstr);
        }
        let relative_offset = archive.2.len() as _;
        archive.2.extend(read(&f).unwrap().into_iter());
        let byte_length = archive.2.len() as u64 - relative_offset;
        archive.1.insert(fstr.to_owned(), par::AssetEntryHeadingPair { relative_offset, byte_length });
    }
    if let Some(ofpath) = args.value_of("ofile") {
        archive.write(&mut File::create(ofpath).unwrap()).unwrap();
    }
    else {
        let foptr = unsafe { libc::fdopen(libc::dup(1), "wb\x00".as_ptr() as *const _) };
        archive.write(&mut NativeOfstream::from_stream_ptr(foptr).unwrap()).unwrap();
    }
}

use std::ptr::NonNull;
struct NativeOfstream(NonNull<libc::FILE>);
impl NativeOfstream {
    pub fn from_stream_ptr(p: *mut libc::FILE) -> Option<Self> {
        NonNull::new(p).map(NativeOfstream)
    }
}
impl Drop for NativeOfstream {
    fn drop(&mut self) {
        unsafe { libc::fclose(self.0.as_ptr()); }
    }
}
impl std::io::Write for NativeOfstream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let written = unsafe { libc::fwrite(buf.as_ptr() as *const _, 1, buf.len() as _, self.0.as_ptr()) };
        return Ok(written);
    }
    fn flush(&mut self) -> std::io::Result<()> {
        let code = unsafe { libc::fflush(self.0.as_ptr()) };
        if code == 0 { Ok(()) } else { Err(std::io::Error::last_os_error()) }
    }
}

use std::path::{Path, PathBuf}; use std::borrow::ToOwned;
fn extract_directory(p: &Path) -> Box<Iterator<Item = PathBuf>> {
    if metadata(p).unwrap().is_dir() {
        Box::new(read_dir(p).unwrap().flat_map(|f| extract_directory(f.unwrap().path().as_path())))
    }
    else {
        Box::new(Some(p.to_owned()).into_iter())
    }
}
