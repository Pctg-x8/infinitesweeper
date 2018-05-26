use bedrock as br;
use appframe::*;
use std::rc::Rc;

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
    g: LateInit<Graphics>, w: LateInit<Rc<MainWindow<E>>>, event_handler: E
}
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
        let g = Graphics::new(self.appname, self.appversion).unwrap();
        let w = MainWindow::new(&format!("{} v{}.{}.{}", self.appname, self.appversion.0, self.appversion.1, self.appversion.2), app);
        w.0.get().show();
        self.g.init(g); self.w.init(w);
    }
}
struct MainWindow<E: EngineEvents + 'static>(LateInit<NativeWindow<MainWindow<E>>>);
impl<E: EngineEvents + 'static> MainWindow<E>
{
    pub fn new(caption: &str, app: &Rc<GUIApplication<Engine<E>>>) -> Rc<Self>
    {
        let this: Rc<_> = MainWindow(LateInit::new()).into();
        this.0.init(NativeWindowBuilder::new(512 * 10 / 16, 512, caption)
            .resizable(false).create_renderable(app, &this).unwrap());
        return this;
    }
}
impl<E: EngineEvents> WindowEventDelegate for MainWindow<E>
{
    type ClientDelegate = Engine<E>;
}

use std::cell::{Ref, RefCell};
struct LateInit<T>(RefCell<Option<T>>);
impl<T> LateInit<T>
{
    fn new() -> Self { LateInit(RefCell::new(None)) }
    fn init(&self, v: T) { *self.0.borrow_mut() = v.into(); }
    fn get(&self) -> Ref<T> { Ref::map(self.0.borrow(), |x| x.as_ref().unwrap()) }
}

pub struct Queue { q: br::Queue, family: u32 }
pub struct Graphics
{
    instance: br::Instance, adapter: br::PhysicalDevice, device: br::Device,
    graphics_queue: Queue
}
impl Graphics
{
    pub fn new(appname: &str, appversion: (u32, u32, u32)) -> br::Result<Self>
    {
        #[cfg(windows)] const VK_KHR_PLATFORM_SURFACE: &'static str = "VK_KHR_win32_surface";
        let instance = br::InstanceBuilder::new(appname, appversion, "Interlude", (2, 0, 0))
            .add_extensions(vec!["VK_KHR_surface", VK_KHR_PLATFORM_SURFACE])
            .create()?;
        let adapter = instance.iter_physical_devices()?.next().unwrap();
        let gqf_index = adapter.queue_family_properties().find_matching_index(br::QueueFlags::GRAPHICS)
            .expect("No graphics queue");
        let qci = br::DeviceQueueCreateInfo(gqf_index, vec![0.0]);
        let device = br::DeviceBuilder::new(&adapter)
            .add_extension("VK_KHR_swapchain").add_queue(qci).create()?;
        
        return Ok(Graphics
        {
            graphics_queue: Queue { q: device.queue(gqf_index, 0), family: gqf_index },
            instance, adapter, device
        });
    }
}
