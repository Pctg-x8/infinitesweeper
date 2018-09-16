
extern crate peridot_serialization_utils;
extern crate clap; extern crate glob; mod par; extern crate libc;
extern crate crc; extern crate lz4; extern crate libflate; extern crate zstd;
use clap::{App, Arg};
use std::fs::{metadata, read_dir, read, File};

fn main() {
    let matcher = App::new("peridot-archiver").version("0.1").author("S.Percentage <Syn.Tri.Naga@gmail.com>")
        .arg(Arg::with_name("ofile").short("o").long("output").value_name("FILE").help("Describes where archive file will be written"))
        .arg(Arg::with_name("ifiled").help("Input File/Directory").required(true).multiple(true))
        .arg(Arg::with_name("cmethod").short("c").long("compress").value_name("METHOD")
            .possible_values(&["lz4", "zlib", "zstd11"]).takes_value(true).help("Describes the compression method"));
    let matches = matcher.get_matches();

    let directory_walker = matches.values_of("ifiled").unwrap()
        .flat_map(|f| if cfg!(windows) && f.contains('*') {
            Box::new(glob::glob(f).unwrap().flat_map(|f| extract_directory(&f.unwrap())))
        }
        else { extract_directory(Path::new(f)) });
    let compression_method = matches.value_of("cmethod").map(|s| match s {
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
    if let Some(ofpath) = matches.value_of("ofile") {
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
