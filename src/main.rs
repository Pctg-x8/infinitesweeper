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

macro_rules! OffsetOf {
    ($t: ty => $m: ident) => {
        unsafe { std::mem::transmute::<_, usize>(&std::mem::transmute::<_, &$t>(0usize).$m) }
    }
}

#[repr(C)]
pub struct ShaderSpecConstants {
    pub screen_aspect_wh: f32,
    pub emboss_thickness: f32
}
impl ShaderSpecConstants {
    pub fn spec_info(&self) -> (Vec<br::vk::VkSpecializationMapEntry>, br::DynamicDataCell) {
        let entries = vec![
            br::vk::VkSpecializationMapEntry {
                constantID: 0, size: std::mem::size_of::<f32>() as _,
                offset: unsafe { std::mem::transmute::<_, usize>(&std::mem::transmute::<_, &Self>(0usize).screen_aspect_wh) as _ }
            }
        ];
        (entries, br::DynamicDataCell::from(self))
    }
    pub fn spec_info_frag(&self) -> (Vec<br::vk::VkSpecializationMapEntry>, br::DynamicDataCell) {
        let entries = vec![
            br::vk::VkSpecializationMapEntry {
                constantID: 0, size: std::mem::size_of::<f32>() as _,
                offset: OffsetOf!(Self => emboss_thickness) as _
            }
        ];
        (entries, br::DynamicDataCell::from(self))
    }
}

struct Game
{
    rp: LateInit<br::RenderPass>, framebuffers: Discardable<Vec<br::Framebuffer>>,
    framebuffer_commands: Discardable<CommandBundle>, pass_gp: LateInit<LayoutedPipeline>,
    res: LateInit<MainResources>
}
impl Game
{
    fn launch()
    {
        Engine::launch("InfiniteMinesweeper", (0, 1, 0), Game
        {
            rp: LateInit::new(), framebuffers: Discardable::new(), framebuffer_commands: Discardable::new(),
            pass_gp: LateInit::new(), res: LateInit::new()
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

        let (mut tb, mut dsub) = (TransferBatch::new(), DescriptorSetUpdateBatch::new());
        let res = MainResources::init(e, &mut tb, &mut dsub).unwrap();
        dsub.submit(&e.graphics_device());
        e.submit_commands(|r|
        {
            let ibs: Vec<_> = e.backbuffers().iter().map(|v| br::ImageMemoryBarrier::new(&br::ImageSubref::color(&v, 0, 0),
                br::ImageLayout::Undefined, br::ImageLayout::PresentSrc)).collect();
            r.pipeline_barrier(br::PipelineStageFlags::TOP_OF_PIPE, br::PipelineStageFlags::BOTTOM_OF_PIPE, false,
                &[], &[], &ibs);
            tb.sink_transfer_commands(r);
            tb.sink_graphics_ready_commands(r);
        }).unwrap();

        let pvp_pass = PvpContainerReader::from_file("assets/shaders/pass.pvp").unwrap().into_container().unwrap();
        let pass_shaders = PvpShaderModules::new(&e.graphics_device(), pvp_pass).unwrap();
        let u0_layout: Rc<_> = br::PipelineLayout::new(&e.graphics_device(), &[&res.dsl_u0], &[]).unwrap().into();
        let screen_spec = ShaderSpecConstants {
            screen_aspect_wh: filling_viewport.width / filling_viewport.height,
            emboss_thickness: 0.05
        };
        let pass_gp = br::GraphicsPipelineBuilder::new(&u0_layout, (&rp, 0))
            .vertex_processing({
                let mut vps = pass_shaders.generate_vps(br::vk::VK_PRIMITIVE_TOPOLOGY_TRIANGLE_STRIP);
                vps.mod_vertex_shader().specinfo = screen_spec.spec_info().into();
                vps.mod_fragment_shader().unwrap().specinfo = screen_spec.spec_info_frag().into();
                vps
            })
            .fixed_viewport_scissors(br::DynamicArrayState::Static(&[filling_viewport]), br::DynamicArrayState::Static(&[framebuffer_size.clone()]))
            .add_attachment_blend(br::AttachmentColorBlendState::noblend())
            .create(&e.graphics_device(), None).unwrap();
        let pass_gp = LayoutedPipeline::combine(pass_gp, &u0_layout);

        let framebuffer_commands = CommandBundle::new(&e.graphics_device(), e.graphics_queue_family_index(), framebuffers.len())
            .expect("Framebuffer CommandBundle");
        for (fb, cb) in framebuffers.iter().zip(framebuffer_commands.iter())
        {
            let mut rec = cb.begin().expect("Beginning Recording commands");
            rec.begin_render_pass(&rp, fb, framebuffer_size.clone(), &[br::ClearValue::Color([0.0; 4])], true);
            pass_gp.bind(&mut rec);
            rec.bind_graphics_descriptor_sets(0, &[res.dsets[0]], &[]);
            res.stack.draw_unit_rect(&res.buffer, &mut rec);
            rec.end_render_pass();
        }
        
        self.rp.init(rp); self.framebuffers.set(framebuffers); self.framebuffer_commands.set(framebuffer_commands);
        self.pass_gp.init(pass_gp); self.res.init(res);
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

#[repr(C)] #[derive(Clone, PartialEq)]
pub struct VertexPlacementUniformData { pub offs: [f32; 2], pub scale: [f32; 2] }

pub struct ResourceStack {
    unit_rect_vb: usize,
    vertex_placement_ub: usize
}
impl ResourceStack {
    pub fn init(bp: &mut BufferPrealloc) -> Self {
        ResourceStack {
            unit_rect_vb: bp.add(BufferContent::vertex::<[[f32; 4]; 4]>()),
            vertex_placement_ub: bp.add(BufferContent::uniform::<[VertexPlacementUniformData; 1]>())
        }
    }
    pub fn init_data(&self, mem: &br::MappedMemoryRange) {
        unsafe {
            mem.get_mut::<[[f32; 4]; 4]>(self.unit_rect_vb).clone_from_slice(&[
                [-0.5, -0.5, 0.0, 0.0], [-0.5, 0.5, 0.0, 1.0],
                [ 0.5, -0.5, 1.0, 0.0], [ 0.5, 0.5, 1.0, 1.0]
            ]);
            mem.get_mut::<[VertexPlacementUniformData; 1]>(self.vertex_placement_ub).clone_from_slice(&[
                VertexPlacementUniformData { offs: [0.0, 0.0], scale: [0.125, 0.125] }
            ]);
        }
    }
    pub fn draw_unit_rect(&self, buf: &br::Buffer, rec: &mut br::CmdRecord) {
        rec.bind_vertex_buffers(0, &[(buf, self.unit_rect_vb)]).draw(4, 1, 0, 0);
    }
}
struct MainResources {
    stack: ResourceStack, buffer: Buffer, dsl_u0: br::DescriptorSetLayout, dpool: br::DescriptorPool, dsets: Vec<br::vk::VkDescriptorSet>
}
impl MainResources {
    fn init(e: &Engine<Game>, transfer_batch: &mut TransferBatch, dsu_batch: &mut DescriptorSetUpdateBatch) -> br::Result<Self> {
        let g = e.graphics();
        let gd = e.graphics_device();

        let mut bp = BufferPrealloc::new(&g);
        let rs = ResourceStack::init(&mut bp);
        let buffer = MemoryBadget::new(&g).alloc_with_buffer(bp.build_transferred()?)?;
        let buffer_upload = MemoryBadget::new(&g).alloc_with_buffer_host_visible(bp.build_upload()?)?;
        buffer_upload.guard_map(bp.total_size(), |m| rs.init_data(m))?;

        let dsl_u0 = br::DescriptorSetLayout::new(&gd, &br::DSLBindings {
            uniform_buffer: (0, 1, br::ShaderStage::VERTEX).into(),
            .. br::DSLBindings::empty()
        })?;
        let dpool = br::DescriptorPool::new(&gd, 1, &[br::DescriptorPoolSize(br::DescriptorType::UniformBuffer, 1)], false)?;
        let dsets = dpool.alloc(&[&dsl_u0])?;
        
        transfer_batch.add_mirroring_buffer(&buffer_upload, &buffer, 0, bp.total_size() as _);
        dsu_batch.write(dsets[0], 0, br::DescriptorUpdateInfo::UniformBuffer(vec![(buffer.native_ptr(), rs.vertex_placement_ub .. bp.total_size())]));
        return Ok(MainResources { stack: rs, buffer, dsl_u0, dpool, dsets });
    }
}
