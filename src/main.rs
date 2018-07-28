//! peridot-cradle-pc

extern crate winapi;
extern crate bedrock;
extern crate libc;
#[macro_use] extern crate log;
extern crate regex;
// #[macro_use] extern crate bitflags;
extern crate peridot_vertex_processing_pack;
extern crate env_logger;
mod peridot;

use std::rc::Rc;
use peridot::Engine;
use std::io::Result as IOResult;
use std::path::PathBuf;

mod glib;

type GameT = glib::Game<PlatformAssetLoader, RenderTargetWindow>;
type EngineT = Engine<GameT, PlatformAssetLoader, RenderTargetWindow>;

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

use bedrock as br;
struct RenderTargetWindow { instance: HINSTANCE, handle: HWND }
impl peridot::PlatformRenderTarget for RenderTargetWindow {
    fn create_surface(&self, vi: &br::Instance, pd: &br::PhysicalDevice, renderer_queue_family: u32)
            -> br::Result<peridot::SurfaceInfo> {
        if !pd.win32_presentation_support(renderer_queue_family) { panic!("Vulkan Presentation is not supported on this platform"); }
        let obj = br::Surface::new_win32(vi, self.instance, self.handle)?;
        if !pd.surface_support(renderer_queue_family, &obj)? { panic!("Vulkan Surface is not supported on this adapter"); }
        return peridot::SurfaceInfo::gather_info(pd, obj);
    }
    fn current_geometry_extent(&self) -> (usize, usize) {
        let mut r: RECT = unsafe { std::mem::zeroed() };
        unsafe { GetClientRect(self.handle, &mut r); }
        return ((r.right - r.left) as _, (r.bottom - r.top) as _);
    }
}

fn main() {
    env_logger::init();

    let hinst = unsafe { GetModuleHandle(std::ptr::null()) };
    let init_caption = std::ffi::CString::new(
        format!("{} v{}.{}.{}", GameT::NAME, GameT::VERSION.0, GameT::VERSION.1, GameT::VERSION.2)
    ).unwrap();
    let wce = WNDCLASSEX {
        cbSize: std::mem::size_of::<WNDCLASSEX>() as _,
        lpszClassName: "peridot.RenderTargetWindow".as_ptr() as *const _,
        lpfnWndProc: Some(window_callback),
        hInstance: hinst,
        .. unsafe { std::mem::zeroed() }
    };
    let wc = unsafe { RegisterClassEx(&wce) };
    if wc == 0 { panic!("Unable to register a Window Class"); }
    let hw = unsafe {
        let ws = WS_OVERLAPPED | WS_SYSMENU | WS_BORDER | WS_MINIMIZEBOX;
        let mut cr0 = RECT { left: 0, top: 0, right: 512 * 10 / 16, bottom: 512 };
        AdjustWindowRectEx(&mut cr0, ws, false, WS_EX_APPWINDOW);
        CreateWindowEx(WS_EX_APPWINDOW, wce.lpszClassName, init_caption.as_ptr(), ws, CW_USEDEFAULT, CW_USEDEFAULT,
            cr0.right - cr0.left, cr0.bottom - cr0.top, std::ptr::null_mut(), std::ptr::null_mut(), hinst, std::ptr::null_mut())
    };
    if hw.is_null() { panic!("Unable to create a Window"); }

    let prt = RenderTargetWindow { instance: hinst, handle: hw };
    let mut ipp = PlatformInputProcessPlugin::new();
    let mut engine = Engine::launch(GameT::NAME, GameT::VERSION, prt, PlatformAssetLoader::new(), &mut ipp)
        .expect("Failed to initialize the Engine");
    
    'app: loop {
        let mut msg = unsafe { std::mem::uninitialized() };
        while unsafe { PeekMessage(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) > 0 } {
            if msg.message == WM_QUIT { break 'app; }
            unsafe { TranslateMessage(&mut msg); DispatchMessage(&mut msg); }
        }
        engine.do_update();
    }
}

extern "system" fn window_callback(h: HWND, msg: UINT, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_DESTROY => unsafe { PostQuitMessage(0); return 0; },
        WM_INPUT => {
            debug!("PlatformInputMessage: {:08x} {:08x}", wp, lp);
            return 0;
        }
        _ => unsafe { DefWindowProc(h, msg, wp, lp) }
    }
}
