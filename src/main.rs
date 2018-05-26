extern crate appframe;
extern crate bedrock;
extern crate libc;
#[macro_use] extern crate log;
extern crate env_logger;

mod peridot; use peridot::*;

fn main()
{
    env_logger::init();
    Engine::launch("InfiniteMinesweeper", (0, 1, 0), ());
}
