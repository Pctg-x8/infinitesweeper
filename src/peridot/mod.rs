use bedrock as br; use bedrock::traits::*;
use appframe::*;
use std::rc::Rc;
use std::borrow::Cow;

mod window; use self::window::{MainWindow, SurfaceInfo, WindowRenderTargets};
#[cfg(debug_assertions)] mod debug; #[cfg(debug_assertions)] use self::debug::DebugReport;

pub trait EngineEvents : Sized
{
    fn init(&self, _e: &Engine<Self>) {}
    fn update(&self, _e: &Engine<Self>) {}
}
impl EngineEvents for () {}
impl<F: Fn(&Engine<F>)> EngineEvents for F { fn update(&self, e: &Engine<Self>) { self(e); } }
pub struct Engine<E: EngineEvents + 'static>
{
    appname: &'static str, appversion: (u32, u32, u32),
    pub(self) g: LateInit<Graphics>, w: LateInit<Rc<MainWindow<E>>>, wrt: Discardable<WindowRenderTargets>,
    event_handler: E
}
type PlatformServer<E> = GUIApplication<Engine<E>>;
impl<E: EngineEvents + 'static> Engine<E>
{
    pub fn launch(appname: &'static str, version: (u32, u32, u32), event_handler: E)
    {
        GUIApplication::run(Engine
        {
            appname, appversion: version, event_handler,
            g: LateInit::new(), w: LateInit::new(), wrt: Discardable::new()
        });
    }
    pub fn graphics(&self) -> Ref<Graphics> { self.g.get() }
    pub fn graphics_device(&self) -> Ref<br::Device> { Ref::map(self.g.get(), |g| &g.device) }
    pub fn backbuffer_format(&self) -> br::vk::VkFormat { self.w.get().backbuffer_format() }
    pub fn backbuffers(&self) -> Ref<[br::ImageView]> { Ref::map(self.wrt.get(), |wrt| wrt.backbuffers()) }
    
    pub fn submit_commands<Gen: FnOnce(&mut br::CmdRecord)>(&self, generator: Gen) -> br::Result<()>
    {
        self.g.get().submit_commands(generator)
    }

    pub(self) fn create_wrt(&self, si: &SurfaceInfo, v: &NativeView<MainWindow<E>>) -> br::Result<()>
    {
        let wrt = WindowRenderTargets::new(&self.g.get(), si, v)?;
        self.wrt.set(wrt); return Ok(());
    }
    pub(self) fn do_update(&self)
    {
        self.event_handler.update(self);
    }
}
impl<E: EngineEvents> EventDelegate for Engine<E>
{
    fn postinit(&self, app: &Rc<PlatformServer<E>>)
    {
        let g = Graphics::new(self.appname, self.appversion).unwrap(); self.g.init(g);
        let w = MainWindow::new(&format!("{} v{}.{}.{}", self.appname, self.appversion.0, self.appversion.1, self.appversion.2),
            512 * 10 / 16, 512, app);
        w.show();
        self.w.init(w);
        self.event_handler.init(self);
    }
}

use std::cell::{Ref, RefCell};
pub struct LateInit<T>(RefCell<Option<T>>);
impl<T> LateInit<T>
{
    pub fn new() -> Self { LateInit(RefCell::new(None)) }
    pub fn init(&self, v: T) { *self.0.borrow_mut() = v.into(); }
    pub fn get(&self) -> Ref<T> { Ref::map(self.0.borrow(), |x| x.as_ref().unwrap()) }
}
pub struct Discardable<T>(RefCell<Option<T>>);
impl<T> Discardable<T>
{
    pub fn new() -> Self { Discardable(RefCell::new(None)) }
    pub fn set(&self, v: T) { *self.0.borrow_mut() = v.into(); }
    pub fn get(&self) -> Ref<T> { Ref::map(self.0.borrow(), |x| x.as_ref().unwrap()) }
    pub fn discard(&self) { *self.0.borrow_mut() = None; }
    pub fn is_available(&self) -> bool { self.0.borrow().is_some() }
}

pub struct Queue { q: br::Queue, family: u32 }
pub struct Graphics
{
    instance: br::Instance, pub(self) adapter: br::PhysicalDevice, device: br::Device,
    graphics_queue: Queue,
    #[cfg(debug_assertions)] _d: DebugReport,
    cp_onetime_submit: br::CommandPool
}
impl Graphics
{
    fn new(appname: &str, appversion: (u32, u32, u32)) -> br::Result<Self>
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
            cp_onetime_submit: br::CommandPool::new(&device, gqf_index, true, false)?,
            graphics_queue: Queue { q: device.queue(gqf_index, 0), family: gqf_index },
            instance, adapter, device,
            #[cfg(debug_assertions)] _d
        });
    }

    pub(self) fn presentation_support_on<S: BedrockRenderingServer>(&self, s: &S) -> bool
    {
        s.presentation_support(&self.adapter, self.graphics_queue.family)
    }
    pub(self) fn create_surface_on<S: BedrockRenderingServer, WE: WindowEventDelegate>(&self, s: &S, v: &NativeView<WE>)
        -> br::Result<br::Surface>
    {
        s.create_surface(v, &self.instance)
    }
    pub(self) fn surface_support(&self, s: &br::Surface) -> br::Result<bool>
    {
        self.adapter.surface_support(self.graphics_queue.family, s)
    }
    
    fn submit_commands<Gen: FnOnce(&mut br::CmdRecord)>(&self, generator: Gen) -> br::Result<()>
    {
        let cb = LocalCommandBundle(self.cp_onetime_submit.alloc(1, true)?, &self.cp_onetime_submit);
        generator(&mut cb[0].begin_once()?);
        self.graphics_queue.q.submit(&[br::SubmissionBatch
        {
            command_buffers: Cow::from(&cb[..]), .. Default::default()
        }], None)?;
        self.graphics_queue.q.wait()
    }
}

struct LocalCommandBundle<'p>(Vec<br::CommandBuffer>, &'p br::CommandPool);
impl<'p> ::std::ops::Deref for LocalCommandBundle<'p>
{
    type Target = [br::CommandBuffer];
    fn deref(&self) -> &[br::CommandBuffer] { &self.0 }
}
impl<'p> Drop for LocalCommandBundle<'p>
{
    fn drop(&mut self) { self.1.free(&self.0[..]); }
}
