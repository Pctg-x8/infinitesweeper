use appframe::*;
use super::*;
use bedrock as br;
use std::rc::{Rc, Weak};

pub struct MainWindow<E: EngineEvents + 'static>
{
    srv: Weak<PlatformServer<E>>, w: LateInit<NativeWindow<MainWindow<E>>>,
    pub(super) surface: LateInit<SurfaceInfo>
}
impl<E: EngineEvents + 'static> MainWindow<E>
{
    pub fn new(caption: &str, width: u16, height: u16, app: &Rc<PlatformServer<E>>) -> Rc<Self>
    {
        let this = Rc::new(MainWindow
        {
            srv: Rc::downgrade(app), w: LateInit::new(), surface: LateInit::new()
        });
        this.w.init(NativeWindowBuilder::new(width, height, caption)
            .resizable(false).create_renderable(app, &this).unwrap());
        return this;
    }
    pub fn show(&self) { self.w.get().show(); }

    pub fn backbuffer_format(&self) -> br::vk::VkFormat { self.surface.get().fmt.format }
}
impl<E: EngineEvents> WindowEventDelegate for MainWindow<E>
{
    type ClientDelegate = Engine<E>;

    fn init_view(&self, view: &NativeView<Self>)
    {
        let srv = self.srv.upgrade().unwrap();
        let surface = SurfaceInfo::new(&srv, view).unwrap();
        srv.event_delegate().create_wrt(&surface, view).unwrap();
        self.surface.init(surface);
    }
    fn render(&self) { self.srv.upgrade().unwrap().event_delegate().do_update(); }
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
    chain: br::Swapchain, bb: Vec<br::ImageView>
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
        return Ok(WindowRenderTargets
        {
            bb: chain.get_images()?.into_iter()
                .map(|x| x.create_view(None, None, &Default::default(), &isr_c0))
                .collect::<Result<_, _>>()?,
            chain
        });
    }

    pub fn backbuffers(&self) -> &[br::ImageView] { &self.bb }
    pub fn acquire_next_backbuffer_index(&self, timeout: Option<u64>, completion_handler: br::CompletionHandler)
        -> br::Result<u32>
    {
        self.chain.acquire_next(timeout, completion_handler)
    }
    pub fn present_on(&self, q: &br::Queue, index: u32, occurence_after: &[&br::Semaphore]) -> br::Result<()>
    {
        self.chain.queue_present(q, index, occurence_after)
    }
}
