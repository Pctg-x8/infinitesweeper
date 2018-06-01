use bedrock as br; use bedrock::traits::*;
use appframe::*;
use std::rc::Rc;
use std::borrow::Cow;
use std::thread::{Thread, Builder as ThreadBuilder};
use std::sync::Arc;

mod window; use self::window::{MainWindow, SurfaceInfo, WindowRenderTargets};
#[cfg(debug_assertions)] mod debug; #[cfg(debug_assertions)] use self::debug::DebugReport;

pub trait EngineEvents : Sized
{
    fn init(&self, _e: &Engine<Self>) {}
    fn update(&self, _e: &Engine<Self>, _on_backbuffer_of: u32) -> br::SubmissionBatch { br::SubmissionBatch::default() }
}
impl EngineEvents for () {}
/*impl<F: Fn(&Engine<F>, u32) -> br::SubmissionBatch> EngineEvents for F
{
    fn update(&self, e: &Engine<Self>, on_backbuffer_of: u32) -> br::SubmissionBatch { self(e, on_backbuffer_of) }
}*/
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
    pub fn graphics_queue_family_index(&self) -> u32 { self.graphics().graphics_queue.family }
    pub fn backbuffer_format(&self) -> br::vk::VkFormat { self.w.get().backbuffer_format() }
    pub fn backbuffers(&self) -> Ref<[br::ImageView]> { Ref::map(self.wrt.get(), |wrt| wrt.backbuffers()) }
    
    pub fn submit_commands<Gen: FnOnce(&mut br::CmdRecord)>(&self, generator: Gen) -> br::Result<()>
    {
        self.g.get().submit_commands(generator)
    }
    pub fn submit_buffered_commands(&self, batches: &[br::SubmissionBatch], fence: &br::Fence) -> br::Result<()>
    {
        self.graphics().graphics_queue.q.submit(batches, Some(fence))
    }

    pub(self) fn create_wrt(&self, si: &SurfaceInfo, v: &NativeView<MainWindow<E>>) -> br::Result<()>
    {
        let wrt = WindowRenderTargets::new(&self.g.get(), si, v)?;
        self.wrt.set(wrt); return Ok(());
    }
    pub(self) fn do_update(&self)
    {
        let g = self.graphics(); let mut wrt = self.wrt.get_mut();
        let bb_index = wrt.acquire_next_backbuffer_index(None, br::CompletionHandler::Device(&g.acquiring_backbuffer))
            .expect("Acquiring available backbuffer index");
        {
            let bbf = wrt.command_completion_for_backbuffer_mut(bb_index as _);
            bbf.wait().expect("Waiting Previous command completion");
            let mut fb_submission = self.event_handler.update(self, bb_index);
            fb_submission.signal_semaphores.to_mut().push(&g.present_ordering);
            fb_submission.wait_semaphores.to_mut().push((&g.acquiring_backbuffer, br::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT));
            self.submit_buffered_commands(&[fb_submission], bbf.object()).expect("CommandBuffer Submission");
            unsafe { bbf.signal(); }
        }
        wrt.present_on(&g.graphics_queue.q, bb_index, &[&g.present_ordering]).expect("Present Submission");
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
impl<E: EngineEvents> Drop for Engine<E> { fn drop(&mut self) { self.graphics().device.wait().unwrap(); } }

use std::cell::{Ref, RefMut, RefCell};
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
    pub fn get_mut(&self) -> RefMut<T> { RefMut::map(self.0.borrow_mut(), |x| x.as_mut().unwrap()) }
    pub fn discard(&self) { *self.0.borrow_mut() = None; }
    pub fn is_available(&self) -> bool { self.0.borrow().is_some() }
}

pub struct Queue { q: br::Queue, family: u32 }
pub struct Graphics
{
    instance: br::Instance, pub(self) adapter: br::PhysicalDevice, device: br::Device,
    graphics_queue: Queue,
    #[cfg(debug_assertions)] _d: DebugReport,
    cp_onetime_submit: br::CommandPool,
    acquiring_backbuffer: br::Semaphore, present_ordering: br::Semaphore
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
            present_ordering: br::Semaphore::new(&device)?,
            acquiring_backbuffer: br::Semaphore::new(&device)?,
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

pub struct CommandBundle(Vec<br::CommandBuffer>, br::CommandPool);
impl ::std::ops::Deref for CommandBundle
{
    type Target = [br::CommandBuffer];
    fn deref(&self) -> &[br::CommandBuffer] { &self.0 }
}
impl Drop for CommandBundle
{
    fn drop(&mut self) { self.1.free(&self.0[..]); }
}
impl CommandBundle
{
    pub fn new(d: &br::Device, queue_family_index: u32, count: usize) -> br::Result<Self>
    {
        let cp = br::CommandPool::new(d, queue_family_index, false, false)?;
        return Ok(CommandBundle(cp.alloc(count as _, true)?, cp));
    }
}

pub enum SubpassDependencyTemplates {}
impl SubpassDependencyTemplates
{
    pub fn to_color_attachment_in(from_subpass: Option<u32>, occurence_subpass: u32, by_region: bool)
        -> br::vk::VkSubpassDependency
    {
        br::vk::VkSubpassDependency
        {
            dstSubpass: occurence_subpass, srcSubpass: from_subpass.unwrap_or(br::vk::VK_SUBPASS_EXTERNAL),
            dstStageMask: br::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT.0,
            dstAccessMask: br::AccessFlags::COLOR_ATTACHMENT.write,
            dependencyFlags: if by_region { br::vk::VK_DEPENDENCY_BY_REGION_BIT } else { 0 },
            srcStageMask: br::PipelineStageFlags::TOP_OF_PIPE.0,
            .. Default::default()
        }
    }
}
