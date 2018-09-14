
extern crate peridot_serialization_utils;
extern crate clap; extern crate glob;
use clap::{App, Arg};
use std::fs::{metadata, read_dir};

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
    for f in directory_walker { println!("input <<= {}", f.display()); }
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
