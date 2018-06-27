//! peridot-cradle-pc

extern crate appframe;
extern crate bedrock;
extern crate libc;
#[macro_use] extern crate log;
extern crate regex;
// #[macro_use] extern crate bitflags;
extern crate peridot_vertex_processing_pack;
extern crate env_logger;
mod peridot;

mod glib;

fn main() {
    env_logger::init(); 
    peridot::Engine::launch(glib::Game::NAME, glib::Game::VERSION, glib::Game::new());
}
