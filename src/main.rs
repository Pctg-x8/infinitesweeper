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

use appframe::*;
use std::rc::{Rc, Weak};
use std::cell::{UnsafeCell, RefCell, Ref, RefMut};
use peridot::Engine;
use std::io::Result as IOResult;

mod glib;

struct App(UnsafeCell<Option<Rc<MainWindow>>>);
impl EventDelegate for App {
    fn postinit(&self, server: &Rc<GUIApplication<Self>>) {
        let w = MainWindow::new(server).expect("Unable to create MainWindow");
        w.inner_ref().show();
        unsafe { *self.0.get() = Some(w); }
    }
}
struct MainWindow {
    server: Weak<GUIApplication<App>>, inner: UnsafeCell<Option<NativeWindow<MainWindow>>>,
    engine: RefCell<Option<Engine<glib::Game>>>
}
impl MainWindow {
    fn new(server: &Rc<GUIApplication<App>>) -> IOResult<Rc<Self>> {
        let this = Rc::new(MainWindow {
            server: Rc::downgrade(server), inner: UnsafeCell::new(None), engine: RefCell::new(None)
        });
        let w = NativeWindowBuilder::new(512 * 10 / 16, 512, glib::Game::NAME)
            .resizable(false).create_renderable(server, &this)?;
        unsafe { *this.inner.get() = Some(w); }
        return Ok(this);
    }
    fn inner_ref(&self) -> &NativeWindow<Self> { unsafe { (*self.inner.get()).as_ref().unwrap() } }
    #[allow(dead_code)]
    fn engine_ref(&self) -> Ref<Engine<glib::Game>> { Ref::map(self.engine.borrow(), |r| r.as_ref().unwrap()) }
    fn engine_mut(&self) -> RefMut<Engine<glib::Game>> { RefMut::map(self.engine.borrow_mut(), |r| r.as_mut().unwrap()) }
}
impl WindowEventDelegate for MainWindow {
    type ClientDelegate = App;

    fn init_view(&self, view: &NativeView<Self>) {
        *self.engine.borrow_mut() = Engine::launch_with_window(glib::Game::NAME, glib::Game::VERSION,
            &self.server.upgrade().unwrap(), view).expect("Failed to initialize the engine").into();
    }
    fn render(&self) { self.engine_mut().do_update(); }
}

fn main() {
    env_logger::init();
    GUIApplication::run(App(UnsafeCell::new(None)));
}
