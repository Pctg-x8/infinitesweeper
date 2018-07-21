#[cfg(not(target_os = "android"))] use appframe::*;
#[cfg(target_os = "android")] use android::ANativeWindow;
use super::*;
use bedrock as br;

use std::mem::{uninitialized, replace, forget};

pub(super) struct SurfaceInfo {
    obj: br::Surface, fmt: br::vk::VkSurfaceFormatKHR, pres_mode: br::PresentMode,
    available_composite_alpha: br::CompositeAlpha
}
impl SurfaceInfo
{
    #[cfg(target_os = "android")]
    pub fn new(g: &Graphics, w: *mut ANativeWindow) -> br::Result<Self> {
        let obj = br::Surface::new_android(&g.instance, w)?;
        if !g.surface_support(&obj)? { panic!("Vulkan Surface is not supported on this adapter"); }
        return Self::gather_info(g, obj);
    }
    #[cfg(not(target_os = "android"))]
    pub fn new<E: EventDelegate, WE: WindowEventDelegate>(s: &GUIApplication<E>, g: &Graphics, w: &NativeView<WE>) -> br::Result<Self>
    {
        if !g.presentation_support_on(s) { panic!("Vulkan Presentation is not supported on this platform"); }
        let obj = g.create_surface_on(s, w)?;
        if !g.surface_support(&obj)? { panic!("Vulkan Surface is not supported on this adapter"); }
        return Self::gather_info(g, obj);
    }

    fn gather_info(g: &Graphics, obj: br::Surface) -> br::Result<Self> {
        let mut fmq = br::FormatQueryPred::new(); fmq.bit(32).components(br::FormatComponents::RGBA).elements(br::ElementType::UNORM);
        let fmt = g.adapter.surface_formats(&obj)?.into_iter().find(|sf| fmq.satisfy(sf.format))
            .expect("No suitable format found");
        let pres_modes = g.adapter.surface_present_modes(&obj)?;
        let &pres_mode = pres_modes.iter().find(|&&m| m == br::PresentMode::FIFO || m == br::PresentMode::Mailbox)
            .unwrap_or(&pres_modes[0]);
        
        let caps = g.adapter.surface_capabilities(&obj)?;
        let available_composite_alpha = if (caps.supportedCompositeAlpha & (br::CompositeAlpha::Inherit as u32)) != 0 {
            br::CompositeAlpha::Inherit
        }
        else {
            br::CompositeAlpha::Opaque
        };
        
        return Ok(SurfaceInfo { obj, fmt, pres_mode, available_composite_alpha });
    }
    pub fn format(&self) -> br::vk::VkFormat { self.fmt.format }
}

pub(super) struct WindowRenderTargets
{
    chain: br::Swapchain, bb: Vec<br::ImageView>, command_completions_for_backbuffer: Vec<StateFence>
}
impl WindowRenderTargets
{
    #[cfg(target_os = "android")]
    pub(super) fn new(g: &Graphics, s: &SurfaceInfo, v: *mut ANativeWindow) -> br::Result<Self>
    {
        let vref = unsafe { &*v };
        let si = g.adapter.surface_capabilities(&s.obj)?;
        let ext = br::Extent2D(
            if si.currentExtent.width == 0xffff_ffff { vref.width() as _ } else { si.currentExtent.width },
            if si.currentExtent.height == 0xffff_ffff { vref.height() as _ } else { si.currentExtent.height });
        let buffer_count = 2.max(si.minImageCount).min(si.maxImageCount);
        let chain = br::SwapchainBuilder::new(&s.obj, buffer_count, &s.fmt, &ext, br::ImageUsage::COLOR_ATTACHMENT)
            .present_mode(s.pres_mode)
            .composite_alpha(s.available_composite_alpha).pre_transform(br::SurfaceTransform::Identity)
            .create(&g.device)?;
        
        let isr_c0 = br::ImageSubresourceRange::color(0, 0);
        let images = chain.get_images()?;
        let (mut bb, mut command_completions_for_backbuffer) = (Vec::with_capacity(images.len()), Vec::with_capacity(images.len()));
        for x in images {
            bb.push(x.create_view(None, None, &Default::default(), &isr_c0)?);
            command_completions_for_backbuffer.push(StateFence::new(&g.device)?);
        }

        return Ok(WindowRenderTargets { command_completions_for_backbuffer, bb, chain });
    }
    #[cfg(not(target_os = "android"))]
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
            command_completions_for_backbuffer.push(StateFence::new(&g.device)?);
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
    pub fn command_completion_for_backbuffer(&self, index: usize) -> &StateFence {
        &self.command_completions_for_backbuffer[index]
    }
    pub fn command_completion_for_backbuffer_mut(&mut self, index: usize) -> &mut StateFence {
        &mut self.command_completions_for_backbuffer[index]
    }
}
impl Drop for WindowRenderTargets
{
    fn drop(&mut self)
    {
        for f in self.command_completions_for_backbuffer.iter_mut() { f.wait().unwrap(); }
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
