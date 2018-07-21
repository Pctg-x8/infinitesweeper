//! peridot-cradle for android platform

#[macro_use] extern crate log;
extern crate libc;
extern crate android_logger;
extern crate bedrock; extern crate peridot_vertex_processing_pack;
extern crate android;

use std::ptr::null_mut;

mod peridot;
mod glib;
use std::cell::RefCell;

struct MainWindow(RefCell<Option<EngineA>>);
impl MainWindow {
    fn new() -> Self { MainWindow(RefCell::new(None)) }
    fn init(&self, app: &android::App) {
        let amgr = unsafe { android::AssetManager::from_ptr((*app.activity).asset_manager).unwrap() };
        *self.0.borrow_mut() = peridot::Engine::launch_with_android_window(GameA::NAME, GameA::VERSION,
            app.window, PlatformAssetLoader::new(amgr)).expect("Failed to initialize the engine").into();
    }
    fn render(&self)
    {
        let mut b = self.0.borrow_mut();
        if let Some(e) = b.as_mut() { e.do_update(); }
    }
}

use android::{AssetManager, Asset, AASSET_MODE_STREAMING, AASSET_MODE_RANDOM};
use std::io::{Result as IOResult, Error as IOError, ErrorKind};
use std::ffi::CString;
struct PlatformAssetLoader { amgr: AssetManager }
impl PlatformAssetLoader {
    fn new(amgr: AssetManager) -> Self { PlatformAssetLoader { amgr } }
}
impl peridot::AssetLoader for PlatformAssetLoader {
    type Asset = Asset;
    type StreamingAsset = Asset;

    fn get(&self, path: &str, ext: &str) -> IOResult<Asset> {
        let mut path_str = path.replace(".", "/"); path_str.push('.'); path_str.push_str(ext);
        let path_str = CString::new(path_str).unwrap();
        self.amgr.open(path_str.as_ptr(), AASSET_MODE_RANDOM).ok_or(IOError::new(ErrorKind::NotFound, ""))
    }
    fn get_streaming(&self, path: &str, ext: &str) -> IOResult<Asset> {
        let mut path_str = path.replace(".", "/"); path_str.push('.'); path_str.push_str(ext);
        let path_str = CString::new(path_str).unwrap();
        self.amgr.open(path_str.as_ptr(), AASSET_MODE_STREAMING).ok_or(IOError::new(ErrorKind::NotFound, ""))
    }
}
type GameA = glib::Game<PlatformAssetLoader>;
type EngineA = peridot::Engine<GameA, PlatformAssetLoader>;

#[no_mangle]
pub extern "C" fn android_main(app: *mut android::App) {
    let app = unsafe { app.as_mut().unwrap() };
    app.on_app_cmd = Some(appcmd_callback);
    let mw = MainWindow::new();
    app.user_data = unsafe { std::mem::transmute(&mw) };

    android_logger::init_once(
        android_logger::Filter::default()
            .with_min_level(log::Level::Trace)
    );
    info!("Launching NativeActivity: {:p}", app);
    std::panic::set_hook(Box::new(|p| {
        error!("Panicking in app: {}", p);
    }));

    'alp: loop {
        let (mut _outfd, mut events, mut source) = (0, 0, null_mut::<android::PollSource>());
        while android::Looper::poll_all(0, &mut _outfd, &mut events, unsafe { std::mem::transmute(&mut source) }) >= 0 {
            if let Some(sref) = unsafe { source.as_mut() } { sref.process(app); }
            if app.destroy_requested != 0 { break 'alp; }
        }
        mw.render();
    }
}

pub extern "C" fn appcmd_callback(app: *mut android::App, cmd: i32) {
    let app = unsafe { app.as_mut().unwrap() };
    let mw = unsafe { std::mem::transmute::<_, *const MainWindow>(app.user_data).as_ref().unwrap() };

    match cmd {
        android::APP_CMD_INIT_WINDOW => {
            trace!("Initializing Window...");
            mw.init(app);
        },
        android::APP_CMD_TERM_WINDOW => {
            trace!("Terminating Window...");
        },
        e => trace!("Unknown Event: {}", e)
    }
}
