extern crate appframe;
extern crate bedrock;
extern crate libc;
#[macro_use] extern crate log;
extern crate env_logger;

use bedrock as br;
mod peridot; use peridot::*;

fn main() { env_logger::init(); Game::launch(); }

struct Game
{
    rp: LateInit<br::RenderPass>, framebuffers: Discardable<Vec<br::Framebuffer>>
}
impl Game
{
    fn launch()
    {
        Engine::launch("InfiniteMinesweeper", (0, 1, 0), Game
        {
            rp: LateInit::new(), framebuffers: Discardable::new()
        });
    }
}
impl EngineEvents for Game
{
    fn init(&self, e: &Engine<Self>)
    {
        info!("Infinite Minesweeper");
        let rp = br::RenderPassBuilder::new()
            .add_attachment(br::AttachmentDescription::new(e.backbuffer_format(), br::ImageLayout::PresentSrc, br::ImageLayout::PresentSrc)
                .load_op(br::LoadOp::Clear).store_op(br::StoreOp::Store))
            .add_subpass(br::SubpassDescription::new().add_color_output(0, br::ImageLayout::ColorAttachmentOpt, None))
            .add_dependency(br::vk::VkSubpassDependency
            {
                srcSubpass: br::vk::VK_SUBPASS_EXTERNAL, dstSubpass: 0,
                srcStageMask: br::PipelineStageFlags::TOP_OF_PIPE.0,
                dstStageMask: br::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT.0,
                dstAccessMask: br::AccessFlags::COLOR_ATTACHMENT.write,
                dependencyFlags: br::vk::VK_DEPENDENCY_BY_REGION_BIT, .. Default::default()
            }).create(&e.graphics_device()).expect("RenderPass");
        let framebuffers: Vec<_> = e.backbuffers().iter()
            .map(|v| br::Framebuffer::new(&rp, &[v], v.size(), 1).expect("Framebuffer")).collect();
        
        e.submit_commands(|r|
        {
            let ibs: Vec<_> = e.backbuffers().iter().map(|v| br::ImageMemoryBarrier::new(&br::ImageSubref::color(&v, 0, 0),
                br::ImageLayout::Undefined, br::ImageLayout::PresentSrc)).collect();
            r.pipeline_barrier(br::PipelineStageFlags::TOP_OF_PIPE, br::PipelineStageFlags::BOTTOM_OF_PIPE, false,
                &[], &[], &ibs);
        }).unwrap();
        
        self.rp.init(rp); self.framebuffers.set(framebuffers);
    }
    fn update(&self, _e: &Engine<Self>)
    {

    }
}
