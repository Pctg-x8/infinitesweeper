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
use std::path::PathBuf;

mod glib;

type GameT = glib::Game<PlatformAssetLoader>;
type EngineT = Engine<GameT, PlatformAssetLoader>;

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
    engine: RefCell<Option<EngineT>>, ipp: RefCell<PlatformInputProcessPlugin>
}
impl MainWindow {
    fn new(server: &Rc<GUIApplication<App>>) -> IOResult<Rc<Self>> {
        let this = Rc::new(MainWindow {
            server: Rc::downgrade(server), inner: UnsafeCell::new(None), engine: RefCell::new(None),
            ipp: PlatformInputProcessPlugin::new().into()
        });
        let w = NativeWindowBuilder::new(512 * 10 / 16, 512, GameT::NAME)
            .resizable(false).create_renderable(server, &this)?;
        unsafe { *this.inner.get() = Some(w); }
        return Ok(this);
    }
    fn inner_ref(&self) -> &NativeWindow<Self> { unsafe { (*self.inner.get()).as_ref().unwrap() } }
    #[allow(dead_code)]
    fn engine_ref(&self) -> Ref<EngineT> { Ref::map(self.engine.borrow(), |r| r.as_ref().unwrap()) }
    fn engine_mut(&self) -> RefMut<EngineT> { RefMut::map(self.engine.borrow_mut(), |r| r.as_mut().unwrap()) }
}
impl WindowEventDelegate for MainWindow {
    type ClientDelegate = App;

    fn init_view(&self, view: &NativeView<Self>) {
        let mut ipp = self.ipp.borrow_mut();
        *self.engine.borrow_mut() = Engine::launch_with_window(GameT::NAME, GameT::VERSION,
            &self.server.upgrade().unwrap(), view, PlatformAssetLoader::new(), &mut *ipp).expect("Failed to initialize the engine").into();
    }
    fn render(&self) { self.engine_mut().do_update(); }
}

struct PlatformInputProcessPlugin { processor: Option<Rc<peridot::InputProcess>> }
impl PlatformInputProcessPlugin {
    fn new() -> Self {
        PlatformInputProcessPlugin { processor: None }
    }
}
impl peridot::InputProcessPlugin for PlatformInputProcessPlugin {
    fn on_start_handle(&mut self, ip: &Rc<peridot::InputProcess>) {
        self.processor = Some(ip.clone());
        info!("Started Handling Inputs...");
    }
}

use std::fs::File;
struct PlatformAssetLoader { base_path: PathBuf }
impl PlatformAssetLoader {
    fn new() -> Self {
        let mut base_path = std::env::current_exe().expect("Couldn't find Path of Executable");
        base_path.pop(); base_path.push("assets");
        return PlatformAssetLoader { base_path }
    }
}
impl peridot::AssetLoader for PlatformAssetLoader {
    type Asset = File;
    type StreamingAsset = File;

    fn get(&self, path: &str, ext: &str) -> IOResult<File> {
        let mut asset_path = self.base_path.clone();
        asset_path.extend(path.split("."));
        asset_path.set_extension(ext);
        debug!("Loading Asset: {}...", asset_path.display());
        return File::open(&asset_path);
    }
    fn get_streaming(&self, path: &str, ext: &str) -> IOResult<File> {
        let mut asset_path = self.base_path.clone();
        asset_path.extend(path.split("."));
        asset_path.set_extension(ext);
        debug!("Loading Asset: {}...", asset_path.display());
        return File::open(&asset_path);
    }
}

fn main() {
    env_logger::init();
    GUIApplication::run(App(UnsafeCell::new(None)));
}
