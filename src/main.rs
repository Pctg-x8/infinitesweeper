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

use bedrock as br;
use winapi::um::{
    winuser::{
        PostQuitMessage, DefWindowProcA as DefWindowProc,
        DispatchMessageA as DispatchMessage, TranslateMessage,
        PeekMessageA as PeekMessage, AdjustWindowRectEx, CreateWindowExA as CreateWindowEx,
        RegisterClassExA as RegisterClassEx, WNDCLASSEXA as WNDCLASSEX,
        GetClientRect, ShowWindow, SW_SHOWNORMAL,
        WM_QUIT, WM_DESTROY, WM_INPUT, PM_REMOVE, CW_USEDEFAULT, WS_EX_APPWINDOW,
        WS_OVERLAPPED, WS_SYSMENU, WS_MINIMIZEBOX, WS_BORDER,
        RAWINPUTDEVICE, RegisterRawInputDevices, RIDEV_INPUTSINK,
        RAWINPUTHEADER, RAWINPUT, RID_INPUT, GetRawInputData, RIM_TYPEMOUSE,
        GetFocus
    },
    libloaderapi::GetModuleHandleA as GetModuleHandle
};
use winapi::shared::{
    minwindef::{LRESULT, LPARAM, WPARAM, UINT, HINSTANCE},
    windef::{HWND, RECT},
    hidusage::{HID_USAGE_PAGE_GENERIC, HID_USAGE_GENERIC_MOUSE}
};

type GameT = glib::Game<PlatformAssetLoader, RenderTargetWindow>;
type EngineT = Engine<GameT, PlatformAssetLoader, RenderTargetWindow>;

struct PlatformInputProcessPlugin { processor: Option<Rc<peridot::InputProcess>> }
impl PlatformInputProcessPlugin {
    fn new() -> Self { PlatformInputProcessPlugin { processor: None } }
    fn register_rawinput(hw: HWND) {
        let rid = [
            RAWINPUTDEVICE {
                usUsagePage: HID_USAGE_PAGE_GENERIC,
                usUsage: HID_USAGE_GENERIC_MOUSE,
                dwFlags: RIDEV_INPUTSINK,
                hwndTarget: hw
            }
        ];
        unsafe { RegisterRawInputDevices(rid.as_ptr(), rid.len() as _, std::mem::size_of::<RAWINPUTDEVICE>() as _); }
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

static mut IPP: *mut PlatformInputProcessPlugin = 0 as _;
fn main() {
    env_logger::init();

    let mut ipp = PlatformInputProcessPlugin::new();
    unsafe { IPP = &mut ipp as *mut _; }

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
        AdjustWindowRectEx(&mut cr0, ws, false as _, WS_EX_APPWINDOW);
        CreateWindowEx(WS_EX_APPWINDOW, wce.lpszClassName, init_caption.as_ptr(), ws, CW_USEDEFAULT, CW_USEDEFAULT,
            cr0.right - cr0.left, cr0.bottom - cr0.top, std::ptr::null_mut(), std::ptr::null_mut(), hinst, std::ptr::null_mut())
    };
    if hw.is_null() { panic!("Unable to create a Window"); }
    PlatformInputProcessPlugin::register_rawinput(hw);

    let prt = RenderTargetWindow { instance: hinst, handle: hw };
    let mut engine = EngineT::launch(GameT::NAME, GameT::VERSION, prt, PlatformAssetLoader::new(), &mut ipp)
        .expect("Failed to initialize the Engine");
    unsafe { ShowWindow(hw, SW_SHOWNORMAL); }
    
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
            if unsafe { GetFocus() != h } { return 0; }

            let mut ri: RAWINPUT = unsafe { std::mem::uninitialized() };
            let mut size = std::mem::size_of::<RAWINPUT>() as _;
            unsafe {
                GetRawInputData(std::mem::transmute(lp), RID_INPUT, std::mem::transmute(&mut ri), &mut size,
                    std::mem::size_of::<RAWINPUTHEADER>() as _);
            }

            match ri.header.dwType {
                RIM_TYPEMOUSE => unsafe {
                    let m = ri.data.mouse();
                    if let Some(ref p) = (*IPP).processor {
                        if (m.usButtonFlags & 0x0400) != 0 {
                            p.dispatch_message(peridot::MouseInputMessage::Wheel(
                                std::mem::transmute::<_, i16>(m.usButtonData) as _));
                        }
                        for n in 0 .. 5 {
                            if (m.usButtonFlags & (0x01 << (n * 2))) != 0 {
                                p.dispatch_message(peridot::MouseInputMessage::ButtonDown(n))
                            }
                            if (m.usButtonFlags & (0x02 << (n * 2))) != 0 {
                                p.dispatch_message(peridot::MouseInputMessage::ButtonUp(n))
                            }
                        }
                        if m.lLastX != 0 || m.lLastY != 0 {
                            if (m.usFlags & 0x01) != 0 {
                                warn!("Absolute Motion does not support: {}, {}", m.lLastX, m.lLastY);
                            }
                            else {
                                p.dispatch_message(peridot::MouseInputMessage::MoveRel(m.lLastX as _, m.lLastY as _));
                            }
                        }
                    }
                }
                t => debug!("PlatformUnknownInputMessage: {}", t)
            }
            return 0;
        }
        _ => unsafe { DefWindowProc(h, msg, wp, lp) }
    }
}
