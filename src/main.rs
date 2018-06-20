extern crate appframe;
extern crate bedrock;
extern crate libc;
#[macro_use] extern crate log;
extern crate env_logger;
extern crate regex;
// #[macro_use] extern crate bitflags;
extern crate peridot_vertex_processing_pack;

use bedrock as br; use bedrock::traits::*;
mod peridot; use peridot::*;
use std::borrow::Cow;
use peridot_vertex_processing_pack::*;
use std::rc::Rc;

fn main() { env_logger::init(); Game::launch(); }

struct Game
{
    rp: LateInit<br::RenderPass>, framebuffers: Discardable<Vec<br::Framebuffer>>,
    framebuffer_commands: Discardable<CommandBundle>, pass_gp: LateInit<LayoutedPipeline>,
    buffer: LateInit<Buffer>
}
impl Game
{
    fn launch()
    {
        Engine::launch("InfiniteMinesweeper", (0, 1, 0), Game
        {
            rp: LateInit::new(), framebuffers: Discardable::new(), framebuffer_commands: Discardable::new(),
            pass_gp: LateInit::new(), buffer: LateInit::new()
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
        let filling_viewport = br::vk::VkViewport {
            x: framebuffer_size.offset.x as _, y: framebuffer_size.offset.y as _,
            width: framebuffer_size.extent.width as _, height: framebuffer_size.extent.height as _,
            minDepth: 0.0, maxDepth: 1.0
        };

        let pvp_pass = PvpContainerReader::from_file("assets/shaders/pass.pvp").unwrap().into_container().unwrap();
        let pass_shaders = PvpShaderModules::new(&e.graphics_device(), pvp_pass).unwrap();
        let empty_layout: Rc<_> = br::PipelineLayout::new(&e.graphics_device(), &[], &[]).unwrap().into();
        let pass_gp = br::GraphicsPipelineBuilder::new(&empty_layout, (&rp, 0))
            .vertex_processing(pass_shaders.generate_vps(br::vk::VK_PRIMITIVE_TOPOLOGY_TRIANGLE_STRIP))
            .fixed_viewport_scissors(br::DynamicArrayState::Static(&[filling_viewport]), br::DynamicArrayState::Static(&[framebuffer_size.clone()]))
            .add_attachment_blend(br::AttachmentColorBlendState::noblend())
            .create(&e.graphics_device(), None).unwrap();
        let pass_gp = LayoutedPipeline::combine(pass_gp, &empty_layout);

        let (restack, buf);
        {
            let g = e.graphics();

            let mut bp = BufferPrealloc::new(&g);
            restack = ResourceStack::init(&mut bp);
            let buf_upload = bp.build_upload().unwrap();
            buf = MemoryBadget::new(&e.graphics()).alloc_with_buffer(bp.build_transferred().unwrap()).unwrap();
            let buf_upload = MemoryBadget::new(&e.graphics()).alloc_with_buffer_host_visible(buf_upload).unwrap();
            buf_upload.guard_map(bp.total_size(), |m| restack.init_data(m)).unwrap();
            let mut tb = TransferBatch::new();
            tb.add_mirroring_buffer(&buf_upload, &buf, 0, bp.total_size() as _);

            e.submit_commands(|r|
            {
                let ibs: Vec<_> = e.backbuffers().iter().map(|v| br::ImageMemoryBarrier::new(&br::ImageSubref::color(&v, 0, 0),
                    br::ImageLayout::Undefined, br::ImageLayout::PresentSrc)).collect();
                r.pipeline_barrier(br::PipelineStageFlags::TOP_OF_PIPE, br::PipelineStageFlags::BOTTOM_OF_PIPE, false,
                    &[], &[], &ibs);
                tb.sink_transfer_commands(r);
                tb.sink_graphics_ready_commands(r);
            }).unwrap();
        }

        let framebuffer_commands = CommandBundle::new(&e.graphics_device(), e.graphics_queue_family_index(), framebuffers.len())
            .expect("Framebuffer CommandBundle");
        for (fb, cb) in framebuffers.iter().zip(framebuffer_commands.iter())
        {
            let mut rec = cb.begin().expect("Beginning Recording commands");
            rec.begin_render_pass(&rp, fb, framebuffer_size.clone(), &[br::ClearValue::Color([0.0; 4])], true);
            pass_gp.bind(&mut rec);
            restack.draw_unit_rect(&buf, &mut rec);
            rec.end_render_pass();
        }
        
        self.rp.init(rp); self.framebuffers.set(framebuffers); self.framebuffer_commands.set(framebuffer_commands);
        self.pass_gp.init(pass_gp); self.buffer.init(buf);
    }
    fn update(&self, e: &Engine<Self>, on_backbuffer_of: u32) -> br::SubmissionBatch
    {
        let bb_index = on_backbuffer_of as usize;
        return br::SubmissionBatch {
            command_buffers: Cow::from(self.framebuffer_commands.get()[bb_index..bb_index+1].to_owned()),
            .. Default::default()
        };
    }
}

pub struct ResourceStack {
    unit_rect_vb: usize
}
impl ResourceStack {
    pub fn init(bp: &mut BufferPrealloc) -> Self {
        ResourceStack {
            unit_rect_vb: bp.add(BufferContent::vertex::<[[f32; 4]; 4]>())
        }
    }
    pub fn init_data(&self, mem: &br::MappedMemoryRange) {
        unsafe {
            mem.get_mut::<[[f32; 4]; 4]>(self.unit_rect_vb).clone_from_slice(&[
                [-0.5, -0.5, 0.0, 1.0], [-0.5, 0.5, 0.0, 1.0],
                [ 0.5, -0.5, 0.0, 1.0], [ 0.5, 0.5, 0.0, 1.0]
            ]);
        }
    }
    pub fn draw_unit_rect(&self, buf: &br::Buffer, rec: &mut br::CmdRecord) {
        rec.bind_vertex_buffers(0, &[(buf, self.unit_rect_vb)]).draw(4, 1, 0, 0);
    }
}
