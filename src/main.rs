extern crate appframe;
extern crate bedrock;
extern crate libc;
#[macro_use] extern crate log;
extern crate env_logger;

use bedrock as br; use bedrock::traits::*;
mod peridot; use peridot::*;
use std::borrow::Cow;

fn main() { env_logger::init(); Game::launch(); }

struct Game
{
    rp: LateInit<br::RenderPass>, framebuffers: Discardable<Vec<br::Framebuffer>>,
    framebuffer_commands: Discardable<CommandBundle>,
    present_ordering: LateInit<br::Semaphore>,
    command_completions: Discardable<Vec<(br::Fence, bool)>>
}
impl Game
{
    fn launch()
    {
        Engine::launch("InfiniteMinesweeper", (0, 1, 0), Game
        {
            rp: LateInit::new(), framebuffers: Discardable::new(), framebuffer_commands: Discardable::new(),
            present_ordering: LateInit::new(), command_completions: Discardable::new()
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
            .add_dependency(SubpassDependencyTemplates::to_color_attachment_in(None, 0, true))
            .create(&e.graphics_device()).expect("RenderPass");
        let framebuffers: Vec<_> = e.backbuffers().iter()
            .map(|v| br::Framebuffer::new(&rp, &[v], v.size(), 1).expect("Framebuffer")).collect();
        let framebuffer_size: br::vk::VkRect2D = br::Extent2D::clone(e.backbuffers()[0].size().as_ref()).into();
        self.command_completions.set(e.backbuffers().iter()
            .map(|_| (br::Fence::new(&e.graphics_device(), false).expect("Fence for RenderCommandCompletion"), false)).collect());
        
        e.submit_commands(|r|
        {
            let ibs: Vec<_> = e.backbuffers().iter().map(|v| br::ImageMemoryBarrier::new(&br::ImageSubref::color(&v, 0, 0),
                br::ImageLayout::Undefined, br::ImageLayout::PresentSrc)).collect();
            r.pipeline_barrier(br::PipelineStageFlags::TOP_OF_PIPE, br::PipelineStageFlags::BOTTOM_OF_PIPE, false,
                &[], &[], &ibs);
        }).unwrap();

        let framebuffer_commands = CommandBundle::new(&e.graphics_device(), e.graphics_queue_family_index(), framebuffers.len())
            .expect("Framebuffer CommandBundle");
        for (fb, cb) in framebuffers.iter().zip(framebuffer_commands.iter())
        {
            let mut rec = cb.begin().expect("Beginning Recording commands");
            rec.begin_render_pass(&rp, fb, framebuffer_size.clone(), &[br::ClearValue::Color([0.0; 4])], true)
                .end_render_pass();
        }
        self.present_ordering.init(br::Semaphore::new(&e.graphics_device()).expect("Semaphore"));
        
        self.rp.init(rp); self.framebuffers.set(framebuffers); self.framebuffer_commands.set(framebuffer_commands);
    }
    fn update(&self, e: &Engine<Self>, on_backbuffer_of: u32)
    {
        let sem_bb = e.semaphore_acquiring_backbuffer();
        let pres_ordering = self.present_ordering.get();
        let bb_index = on_backbuffer_of as usize;
        if self.command_completions.get()[bb_index].1
        {
            self.command_completions.get()[bb_index].0.wait().expect("Waiting Last Command Completion");
            self.command_completions.get()[bb_index].0.reset().expect("Resetting Completion Fence");
        }
        let ref mut last_command_completion_pair = self.command_completions.get_mut()[bb_index];
        e.submit_buffered_commands(&[br::SubmissionBatch
        {
            command_buffers: Cow::from(&self.framebuffer_commands.get()[bb_index..bb_index+1]),
            wait_semaphores: Cow::Borrowed(&[(&sem_bb, br::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)]),
            signal_semaphores: Cow::Borrowed(&[&pres_ordering]),
            .. Default::default()
        }], &last_command_completion_pair.0).expect("CommandBuffer Submission");
        last_command_completion_pair.1 = true;
        e.present(on_backbuffer_of, &[&pres_ordering]).expect("Present Submission");
    }
}
impl Drop for Game
{
    fn drop(&mut self)
    {
        for f in self.command_completions.get().iter().filter(|&(_, b)| *b).map(|&(ref a, _)| a)
        {
            f.wait().unwrap();
        }
    }
}
