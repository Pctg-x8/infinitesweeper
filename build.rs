fn main() {
    if cfg!(windows) {
        let mut cd = std::env::current_dir().unwrap();
        cd.push("spvlibs");
        println!("cargo:rustc-link-search=static={}", cd.display());
    }
}