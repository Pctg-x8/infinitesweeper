use bedrock as br;
use appframe::*;
use std::rc::Rc;

mod window; use self::window::MainWindow;
#[cfg(debug_assertions)] mod debug; #[cfg(debug_assertions)] use self::debug::DebugReport;

pub trait EngineEvents
{
    fn init(&self) {}
    fn update(&self) {}
}
impl EngineEvents for () {}
impl<F: Fn()> EngineEvents for F { fn update(&self) { self(); } }
pub struct Engine<E: EngineEvents + 'static>
{
    appname: &'static str, appversion: (u32, u32, u32),
    pub(self) g: LateInit<Graphics>, w: LateInit<Rc<MainWindow<E>>>, event_handler: E
}
type PlatformServer<E> = GUIApplication<Engine<E>>;
impl<E: EngineEvents + 'static> Engine<E>
{
    pub fn launch(appname: &'static str, version: (u32, u32, u32), event_handler: E)
    {
        GUIApplication::run(Engine
        {
            appname, appversion: version, event_handler,
            g: LateInit::new(), w: LateInit::new()
        });
    }
}
impl<E: EngineEvents> EventDelegate for Engine<E>
{
    fn postinit(&self, app: &Rc<GUIApplication<Engine<E>>>)
    {
        let g = Graphics::new(self.appname, self.appversion).unwrap(); self.g.init(g);
        let w = MainWindow::new(&format!("{} v{}.{}.{}", self.appname, self.appversion.0, self.appversion.1, self.appversion.2),
            512 * 10 / 16, 512, app);
        w.show();
        self.w.init(w);
    }
}

use std::cell::{Ref, RefCell};
struct LateInit<T>(RefCell<Option<T>>);
impl<T> LateInit<T>
{
    fn new() -> Self { LateInit(RefCell::new(None)) }
    fn init(&self, v: T) { *self.0.borrow_mut() = v.into(); }
    fn get(&self) -> Ref<T> { Ref::map(self.0.borrow(), |x| x.as_ref().unwrap()) }
}
struct Discardable<T>(RefCell<Option<T>>);
impl<T> Discardable<T>
{
    fn new() -> Self { Discardable(RefCell::new(None)) }
    fn set(&self, v: T) { *self.0.borrow_mut() = v.into(); }
    fn get(&self) -> Ref<T> { Ref::map(self.0.borrow(), |x| x.as_ref().unwrap()) }
    fn discard(&self) { *self.0.borrow_mut() = None; }
    fn is_available(&self) -> bool { self.0.borrow().is_some() }
}

pub struct Queue { q: br::Queue, family: u32 }
pub struct Graphics
{
    instance: br::Instance, pub(self) adapter: br::PhysicalDevice, device: br::Device,
    graphics_queue: Queue,
    #[cfg(debug_assertions)] _d: DebugReport
}
impl Graphics
{
    pub fn new(appname: &str, appversion: (u32, u32, u32)) -> br::Result<Self>
    {
        #[cfg(windows)] const VK_KHR_PLATFORM_SURFACE: &'static str = "VK_KHR_win32_surface";
        let mut ib = br::InstanceBuilder::new(appname, appversion, "Interlude2:Peridot", (0, 1, 0));
        ib.add_extensions(vec!["VK_KHR_surface", VK_KHR_PLATFORM_SURFACE]);
        #[cfg(debug_assertions)] ib.add_extension("VK_EXT_debug_report").add_layer("VK_LAYER_LUNARG_standard_validation");
        let instance = ib.create()?;
        #[cfg(debug_assertions)] let _d = DebugReport::new(&instance)?;
        #[cfg(debug_assertions)] debug!("Debug reporting activated");
        let adapter = instance.iter_physical_devices()?.next().unwrap();
        let gqf_index = adapter.queue_family_properties().find_matching_index(br::QueueFlags::GRAPHICS)
            .expect("No graphics queue");
        let qci = br::DeviceQueueCreateInfo(gqf_index, vec![0.0]);
        let device = {
            let mut db = br::DeviceBuilder::new(&adapter);
            db.add_extension("VK_KHR_swapchain").add_queue(qci);
            #[cfg(debug_assertions)] db.add_layer("VK_LAYER_LUNARG_standard_validation");
            db.create()?
        };
        
        return Ok(Graphics
        {
            graphics_queue: Queue { q: device.queue(gqf_index, 0), family: gqf_index },
            instance, adapter, device,
            #[cfg(debug_assertions)] _d
        });
    }

    pub fn presentation_support_on<S: BedrockRenderingServer>(&self, s: &S) -> bool
    {
        s.presentation_support(&self.adapter, self.graphics_queue.family)
    }
    pub fn create_surface_on<S: BedrockRenderingServer, WE: WindowEventDelegate>(&self, s: &S, v: &NativeView<WE>)
        -> br::Result<br::Surface>
    {
        s.create_surface(v, &self.instance)
    }
    pub fn surface_support(&self, s: &br::Surface) -> br::Result<bool>
    {
        self.adapter.surface_support(self.graphics_queue.family, s)
    }
}
