use appframe::*;
use super::*;
use bedrock as br;
use std::rc::{Rc, Weak};

use std::mem::{uninitialized, replace, forget};
use std::cell::{UnsafeCell, RefMut, RefCell};
use std::ptr::null_mut;

pub trait RenderWindowInterface {
    fn backbuffer_format(&self) -> br::vk::VkFormat;
    fn backbuffers(&self) -> &[br::ImageView];
    fn acquire_next_backbuffer_index(&self, timeout: Option<u64>, completion_handler: br::CompletionHandler) -> br::Result<u32>;
    fn present_on(&self, q: &br::Queue, index: u32, occurence_after: &[&br::Semaphore]) -> br::Result<()>;
    fn command_completion_for_backbuffer_mut(&self, index: usize) -> RefMut<StateFence>;
}

pub struct MainWindow<E: EngineEvents + 'static> {
    srv: Weak<PlatformServer<E>>, w: LateInit<NativeWindow<MainWindow<E>>>,
    surface_ptr: UnsafeCell<Option<SurfaceInfo>>, wrt_ptr: UnsafeCell<Option<WindowRenderTargets>>
}
impl<E: EngineEvents + 'static> MainWindow<E> {
    pub fn new(caption: &str, width: u16, height: u16, app: &Rc<PlatformServer<E>>) -> Rc<Self> {
        let this = Rc::new(MainWindow {
            srv: Rc::downgrade(app), w: LateInit::new(), surface_ptr: UnsafeCell::new(None),
            wrt_ptr: UnsafeCell::new(None)
        });
        this.w.init(NativeWindowBuilder::new(width, height, caption).resizable(false)
            .create_renderable(app, &this).unwrap());
        return this;
    }
    pub fn show(&self) { self.w.get().show(); }

    fn wrt_ref(&self) -> &WindowRenderTargets { unsafe { (*self.wrt_ptr.get()).as_ref().unwrap() } }
}
impl<E: EngineEvents + 'static> WindowEventDelegate for MainWindow<E> {
    type ClientDelegate = Engine<E>;
    fn init_view(&self, view: &NativeView<Self>) {
        let srv = self.srv.upgrade().unwrap();
        let surface = SurfaceInfo::new(&srv, view).unwrap();
        let wrt = WindowRenderTargets::new(&srv.event_delegate().graphics(), &surface, view).unwrap();
        unsafe {
            *self.wrt_ptr.get() = Some(wrt);
            *self.surface_ptr.get() = Some(surface);
        }
    }
    fn render(&self) { if let Some(srv) = self.srv.upgrade() { srv.event_delegate().do_update(); } }
}
/*impl<E: EngineEvents + 'static> Drop for MainWindow<E> {
    fn drop(&mut self) {
        // drop raw pointers
        if !self.wrt_ptr.is_null() { unsafe { drop(Box::from_raw(self.wrt_ptr)); } self.wrt_ptr = null_mut(); }
        if !self.surface_ptr.is_null() { unsafe { drop(Box::from_raw(self.surface_ptr)); } self.surface_ptr = null_mut(); }
    }
}*/
impl<E: EngineEvents + 'static> RenderWindowInterface for MainWindow<E> {
    fn backbuffer_format(&self) -> br::vk::VkFormat { unsafe { (*self.surface_ptr.get()).as_ref().unwrap().fmt.format } }
    fn backbuffers(&self) -> &[br::ImageView] { &self.wrt_ref().bb }
    fn acquire_next_backbuffer_index(&self, timeout: Option<u64>, completion_handler: br::CompletionHandler) -> br::Result<u32> {
        self.wrt_ref().acquire_next_backbuffer_index(timeout, completion_handler)
    }
    fn present_on(&self, q: &br::Queue, index: u32, occurence_after: &[&br::Semaphore]) -> br::Result<()> {
        self.wrt_ref().present_on(q, index, occurence_after)
    }
    fn command_completion_for_backbuffer_mut(&self, index: usize) -> RefMut<StateFence> {
        self.wrt_ref().command_completion_for_backbuffer_mut(index)
    }
}

pub(super) struct SurfaceInfo { obj: br::Surface, fmt: br::vk::VkSurfaceFormatKHR, pres_mode: br::PresentMode }
impl SurfaceInfo
{
    pub fn new<E: EngineEvents>(s: &PlatformServer<E>, w: &NativeView<MainWindow<E>>) -> br::Result<Self>
    {
        let g = s.event_delegate().g.get();

        if !g.presentation_support_on(s) { panic!("Vulkan Presentation is not supported on this platform"); }
        let obj = g.create_surface_on(s, w)?;
        if !g.surface_support(&obj)? { panic!("Vulkan Surface is not supported on this adapter"); }

        let mut fmq = br::FormatQueryPred::new(); fmq.bit(32).components(br::FormatComponents::RGBA).elements(br::ElementType::UNORM);
        let fmt = g.adapter.surface_formats(&obj)?.into_iter().find(|sf| fmq.satisfy(sf.format))
            .expect("No suitable format found");
        let pres_modes = g.adapter.surface_present_modes(&obj)?;
        let &pres_mode = pres_modes.iter().find(|&&m| m == br::PresentMode::FIFO || m == br::PresentMode::Mailbox)
            .unwrap_or(&pres_modes[0]);
        
        return Ok(SurfaceInfo { obj, fmt, pres_mode });
    }
}

pub(super) struct WindowRenderTargets
{
    chain: br::Swapchain, bb: Vec<br::ImageView>, command_completions_for_backbuffer: Vec<RefCell<StateFence>>
}
impl WindowRenderTargets
{
    pub(super) fn new<WE: WindowEventDelegate>(g: &Graphics, s: &SurfaceInfo, v: &NativeView<WE>) -> br::Result<Self>
    {
        let si = g.adapter.surface_capabilities(&s.obj)?;
        let ext = br::Extent2D(
            if si.currentExtent.width == 0xffff_ffff { v.width() as _ } else { si.currentExtent.width },
            if si.currentExtent.height == 0xffff_ffff { v.height() as _ } else { si.currentExtent.height });
        let buffer_count = 2.max(si.minImageCount).min(si.maxImageCount);
        let chain = br::SwapchainBuilder::new(&s.obj, buffer_count, &s.fmt, &ext, br::ImageUsage::COLOR_ATTACHMENT)
            .present_mode(s.pres_mode)
            .composite_alpha(br::CompositeAlpha::Opaque).pre_transform(br::SurfaceTransform::Identity)
            .create(&g.device)?;
        
        let isr_c0 = br::ImageSubresourceRange::color(0, 0);
        let images = chain.get_images()?;
        let (mut bb, mut command_completions_for_backbuffer) = (Vec::with_capacity(images.len()), Vec::with_capacity(images.len()));
        for x in images {
            bb.push(x.create_view(None, None, &Default::default(), &isr_c0)?);
            command_completions_for_backbuffer.push(StateFence::new(&g.device)?.into());
        }

        return Ok(WindowRenderTargets { command_completions_for_backbuffer, bb, chain });
    }

    pub fn backbuffers(&self) -> &[br::ImageView] { &self.bb }
    pub fn acquire_next_backbuffer_index(&self, timeout: Option<u64>, completion_handler: br::CompletionHandler)
            -> br::Result<u32> {
        self.chain.acquire_next(timeout, completion_handler)
    }
    pub fn present_on(&self, q: &br::Queue, index: u32, occurence_after: &[&br::Semaphore]) -> br::Result<()> {
        self.chain.queue_present(q, index, occurence_after)
    }
    pub fn command_completion_for_backbuffer_mut(&self, index: usize) -> RefMut<StateFence> {
        self.command_completions_for_backbuffer[index].borrow_mut()
    }
}
impl Drop for WindowRenderTargets
{
    fn drop(&mut self)
    {
        for f in self.command_completions_for_backbuffer.iter() { f.borrow_mut().wait().unwrap(); }
    }
}

pub enum StateFence { Signaled(br::Fence), Unsignaled(br::Fence) }
impl StateFence {
    pub fn new(d: &br::Device) -> br::Result<Self> { br::Fence::new(d, false).map(StateFence::Unsignaled) }
    /// must be coherent with background API
    pub unsafe fn signal(&mut self) {
        let unsafe_ = replace(self, uninitialized());
        forget(replace(self, StateFence::Signaled(unsafe_.take_object())));
    }
    /// must be coherent with background API
    unsafe fn unsignal(&mut self) {
        let unsafe_ = replace(self, uninitialized());
        forget(replace(self, StateFence::Unsignaled(unsafe_.take_object())));
    }

    pub fn wait(&mut self) -> br::Result<()> {
        if let StateFence::Signaled(ref f) = *self { f.wait()?; f.reset()?; }
        unsafe { self.unsignal(); } return Ok(());
    }

    pub fn object(&self) -> &br::Fence { match *self { StateFence::Signaled(ref f) | StateFence::Unsignaled(ref f) => f } }
    fn take_object(self) -> br::Fence { match self { StateFence::Signaled(f) | StateFence::Unsignaled(f) => f } }
}
