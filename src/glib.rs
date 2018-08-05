/*extern crate appframe;
extern crate bedrock;
extern crate libc;
#[macro_use] extern crate log;
extern crate regex;
// #[macro_use] extern crate bitflags;
extern crate peridot_vertex_processing_pack;*/

use bedrock as br; use bedrock::traits::*;
use peridot::*;
use std::borrow::Cow;
use peridot_vertex_processing_pack::*;
use std::rc::Rc;
use std::marker::PhantomData;
use std::cell::RefCell;

// fn main() { env_logger::init(); Game::launch(); }

const CHUNK_SIZE: usize = 16;

use std::mem::{transmute, size_of};
macro_rules! OffsetOf {
    ($t: ty => $m: ident) => {
        unsafe { transmute::<_, usize>(&transmute::<_, &$t>(0usize).$m) }
    }
}

#[repr(C)]
pub struct ShaderSpecConstants {
    pub screen_aspect_wh: f32,
    pub emboss_thickness: f32,
}
impl ShaderSpecConstants {
    pub fn spec_info(&self) -> (Vec<br::vk::VkSpecializationMapEntry>, br::DynamicDataCell) {
        let entries = vec![
            br::vk::VkSpecializationMapEntry {
                constantID: 0, size: size_of::<f32>() as _, offset: OffsetOf!(Self => screen_aspect_wh) as _
            }
        ];
        (entries, br::DynamicDataCell::from(self))
    }
    pub fn spec_info_frag(&self) -> (Vec<br::vk::VkSpecializationMapEntry>, br::DynamicDataCell) {
        let entries = vec![
            br::vk::VkSpecializationMapEntry {
                constantID: 0, size: size_of::<f32>() as _, offset: OffsetOf!(Self => emboss_thickness) as _
            }
        ];
        (entries, br::DynamicDataCell::from(self))
    }
}

#[allow(dead_code)]
pub struct Game<AL: AssetLoader, PRT: PlatformRenderTarget>
{
    rp: br::RenderPass, framebuffers: Vec<br::Framebuffer>,
    framebuffer_commands: CommandBundle, pass_gp: LayoutedPipeline,
    res: MainResources, update_commands: Vec<CommandBundle>, render_offset: RefCell<[f32; 2]>,
    _p: PhantomData<(*const AL, *const PRT)>
}
impl<AL: AssetLoader, PRT: PlatformRenderTarget> Game<AL, PRT> {
    pub const NAME: &'static str = "Infinitesweeper";
    pub const VERSION: (u32, u32, u32) = (0, 1, 0);
}
impl<AL: AssetLoader, PRT: PlatformRenderTarget> EngineEvents<AL, PRT> for Game<AL, PRT> {
    fn init(e: &Engine<Self, AL, PRT>) -> Self
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
        e.submit_commands(|r| {
            let ibs: Vec<_> = e.backbuffers().iter().map(|v| br::ImageMemoryBarrier::new(&br::ImageSubref::color(&v, 0, 0),
                br::ImageLayout::Undefined, br::ImageLayout::PresentSrc)).collect();
            r.pipeline_barrier(br::PipelineStageFlags::TOP_OF_PIPE, br::PipelineStageFlags::BOTTOM_OF_PIPE, false,
                &[], &[], &ibs);
            tb.sink_transfer_commands(r);
            tb.sink_graphics_ready_commands(r);
        }).unwrap();

        let pvp_pass: PvpContainer = e.load("shaders.pass").expect("Asset not found");
        let pass_shaders = PvpShaderModules::new(&e.graphics_device(), pvp_pass).unwrap();
        let u0_layout: Rc<_> = br::PipelineLayout::new(&e.graphics_device(), &[&res.dsl_u0],
            &[(br::ShaderStage::VERTEX, 0 .. size_of::<VertexPlacementUniformData>() as _)]).unwrap().into();
        let screen_spec = ShaderSpecConstants {
            screen_aspect_wh: filling_viewport.width / filling_viewport.height,
            emboss_thickness: 0.05
        };
        let pass_gp = br::GraphicsPipelineBuilder::new(&u0_layout, (&rp, 0))
            .vertex_processing({
                let mut vps = pass_shaders.generate_vps(br::vk::VK_PRIMITIVE_TOPOLOGY_TRIANGLE_LIST);
                vps.mod_vertex_shader().specinfo = screen_spec.spec_info().into();
                vps.mod_fragment_shader().unwrap().specinfo = screen_spec.spec_info_frag().into();
                vps
            })
            .fixed_viewport_scissors(br::DynamicArrayState::Static(&[filling_viewport]), br::DynamicArrayState::Static(&[framebuffer_size.clone()]))
            .add_attachment_blend(br::AttachmentColorBlendState::noblend())
            .create(&e.graphics_device(), None).unwrap();
        let pass_gp = LayoutedPipeline::combine(pass_gp, &u0_layout);

        let framebuffer_commands = CommandBundle::new(&e.graphics(), CBSubmissionType::Graphics, framebuffers.len())
            .expect("Framebuffer CommandBundle");
        for (fb, cb) in framebuffers.iter().zip(framebuffer_commands.iter()) {
            let mut rec = cb.begin().expect("Beginning Recording commands");
            rec.begin_render_pass(&rp, fb, framebuffer_size.clone(), &[br::ClearValue::Color([0.0; 4])], true);
            pass_gp.bind(&mut rec);
            res.stack.setup_for_draw_chunked_rects(&res.buffer, &mut rec);
            rec.bind_graphics_descriptor_sets(0, &[res.dset_render_offset], &[]);
            for v in VPUD {
                rec .push_graphics_constant(br::ShaderStage::VERTEX, 0, v)
                    .draw_indexed((6 * CHUNK_SIZE * CHUNK_SIZE) as _, 1, 0, 0, 0);
            }
            rec.end_render_pass();
        }
        let mut update_commands = Vec::with_capacity(framebuffers.len());
        for _ in 0 .. framebuffers.len() {
            update_commands.push(CommandBundle::new(&e.graphics(), CBSubmissionType::Transfer, 1)
                .expect("Updating CommandBundle"));
        }
        /*{
            let _ = update_commands[0].begin().expect("Beginning Recording commands");
        }*/

        return Game {
            rp, framebuffers, framebuffer_commands, update_commands, pass_gp, res, _p: PhantomData,
            render_offset: [0.0; 2].into()
        };
    }
    fn update(&self, e: &Engine<Self, AL, PRT>, on_backbuffer_of: u32) -> (Option<br::SubmissionBatch>, br::SubmissionBatch) {
        let mut tfb = TransferBatch::new();
        self.res.update_pfsbuffer(|m| unsafe {
            if e.input().plane_touching() {
                let md = e.input().plane_delta_move();
                if md.0 != 0 || md.1 != 0 {
                    self.render_offset.borrow_mut()[0] += 0.125 * md.0 as f32;
                    self.render_offset.borrow_mut()[1] += 0.125 * md.1 as f32;
                    m.get_mut::<[f32; 2]>(self.res.pfsstack.render_offset_ub).copy_from_slice(&self.render_offset.borrow()[..]);
                    self.res.pfsstack.commit_render_offset_changes(&self.res.buffer, self.res.stack.render_offset_ub as _, &mut tfb);
                }
            }
        }).unwrap();

        let bb_index = on_backbuffer_of as usize;
        if !tfb.is_empty() {
            self.update_commands[bb_index].reset().unwrap();
            let mut rec = self.update_commands[bb_index][0].begin().expect("Updating UpdateCB");
            tfb.sink_transfer_commands(&mut rec);
            tfb.sink_graphics_ready_commands(&mut rec);
            return (Some(br::SubmissionBatch {
                command_buffers: Cow::Borrowed(&self.update_commands[bb_index]), .. Default::default()
            }), br::SubmissionBatch {
                command_buffers: Cow::Borrowed(&self.framebuffer_commands[bb_index..bb_index+1]),
                .. Default::default()
            });
        }
        else {
            return (None, br::SubmissionBatch {
                command_buffers: Cow::Borrowed(&self.framebuffer_commands[bb_index..bb_index+1]),
                .. Default::default()
            });
        }
    }
}

#[repr(C)] #[derive(Clone, PartialEq)]
pub struct VertexPlacementUniformData { pub offs: [f32; 2], pub scale: [f32; 2], pub chunk_offs: [f32; 2] }

static VPUD: &[VertexPlacementUniformData] = &[
    VertexPlacementUniformData { offs: [0.0, 0.0], scale: [0.125, 0.125], chunk_offs: [  0.0,   0.0] },
    VertexPlacementUniformData { offs: [0.0, 0.0], scale: [0.125, 0.125], chunk_offs: [-16.0,   0.0] },
    VertexPlacementUniformData { offs: [0.0, 0.0], scale: [0.125, 0.125], chunk_offs: [  0.0, -16.0] },
    VertexPlacementUniformData { offs: [0.0, 0.0], scale: [0.125, 0.125], chunk_offs: [-16.0, -16.0] },
    VertexPlacementUniformData { offs: [0.0, 0.0], scale: [0.125, 0.125], chunk_offs: [  0.0,  16.0] },
    VertexPlacementUniformData { offs: [0.0, 0.0], scale: [0.125, 0.125], chunk_offs: [-16.0, -16.0] }
];

pub struct PerFrameStagingResourceStack {
    buffer: Buffer,
    render_offset_ub: usize,
    total_size: usize
}
impl PerFrameStagingResourceStack {
    pub fn init(g: &Graphics) -> Self {
        let mut bp = BufferPrealloc::new(g);
        let render_offset_ub = bp.add(BufferContent::uniform::<[f32; 2]>());
        let buffer = MemoryBadget::new(g)
            .alloc_with_buffer_host_visible(bp.build_upload().expect("Building BufferInfo"))
            .expect("Building Resources");

        return PerFrameStagingResourceStack {
            buffer, render_offset_ub,
            total_size: bp.total_size()
        };
    }
    pub fn commit_render_offset_changes(&self, dest_buffer: &Buffer, dest_offs: u64, tb: &mut TransferBatch) {
        tb.add_copying_buffer((&self.buffer, self.render_offset_ub as _),
            (&dest_buffer, dest_offs), size_of::<[f32; 2]>() as _);
        tb.add_buffer_graphics_ready(br::PipelineStageFlags::VERTEX_SHADER, &dest_buffer,
            dest_offs .. dest_offs + size_of::<[f32; 2]>() as u64,
            br::AccessFlags::UNIFORM_READ);
    }
}
pub struct ResourceStack {
    chunked_rects_vb: usize, chunked_rects_ib: usize, render_offset_ub: usize
}
impl ResourceStack {
    pub fn init(bp: &mut BufferPrealloc) -> Self {
        ResourceStack {
            chunked_rects_vb: bp.add(BufferContent::vertex::<[[f32; 4]; 4 * CHUNK_SIZE * CHUNK_SIZE]>()),
            chunked_rects_ib: bp.add(BufferContent::index::<[u16; 6 * CHUNK_SIZE * CHUNK_SIZE]>()),
            render_offset_ub: bp.add(BufferContent::uniform::<[f32; 2]>())
        }
    }
    pub fn init_data(&self, mem: &br::MappedMemoryRange) {
        unsafe {
            Self::init_chunk_rects(mem.get_mut(self.chunked_rects_vb), mem.get_mut(self.chunked_rects_ib));
            mem.get_mut::<[f32; 2]>(self.render_offset_ub).copy_from_slice(&[0.0, 0.0]);
        }
    }
    fn init_chunk_rects(vertices: &mut [[f32; 4]; 4 * CHUNK_SIZE * CHUNK_SIZE], indices: &mut [u16; 6 * CHUNK_SIZE * CHUNK_SIZE]) {
        for (x, y) in (0 .. CHUNK_SIZE).flat_map(|y| (0 .. CHUNK_SIZE).map(move |x| (x, y))) {
            let flat = x + y * CHUNK_SIZE;
            vertices[flat * 4 + 0] = [x as f32 - 0.5, y as f32 - 0.5, 0.0, 0.0];
            vertices[flat * 4 + 1] = [x as f32 - 0.5, y as f32 + 0.5, 0.0, 1.0];
            vertices[flat * 4 + 2] = [x as f32 + 0.5, y as f32 - 0.5, 1.0, 0.0];
            vertices[flat * 4 + 3] = [x as f32 + 0.5, y as f32 + 0.5, 1.0, 1.0];
            indices[flat * 6 + 0 .. flat * 6 + 6].copy_from_slice(&[
                flat as u16 * 4 + 0, flat as u16 * 4 + 1, flat as u16 * 4 + 2,
                flat as u16 * 4 + 2, flat as u16 * 4 + 1, flat as u16 * 4 + 3
            ]);
        }
    }
    pub fn setup_for_draw_chunked_rects(&self, buf: &br::Buffer, rec: &mut br::CmdRecord) {
        rec .bind_vertex_buffers(0, &[(buf, self.chunked_rects_vb)])
            .bind_index_buffer(&buf, self.chunked_rects_ib, br::IndexType::U16);
    }
    pub fn draw_chunked_rects(&self, buf: &br::Buffer, rec: &mut br::CmdRecord) {
        self.setup_for_draw_chunked_rects(buf, rec);
        rec.draw_indexed((6 * CHUNK_SIZE * CHUNK_SIZE) as _, 1, 0, 0, 0);
    }
}
struct MainResources {
    stack: ResourceStack, buffer: Buffer, dsl_u0: br::DescriptorSetLayout, _dpool: br::DescriptorPool, dset_render_offset: br::vk::VkDescriptorSet,
    pfsstack: PerFrameStagingResourceStack
}
impl MainResources {
    fn init<AL: AssetLoader, PRT: PlatformRenderTarget>(e: &Engine<Game<AL, PRT>, AL, PRT>,
            transfer_batch: &mut TransferBatch, dsu_batch: &mut DescriptorSetUpdateBatch)
            -> br::Result<Self> {
        let g = e.graphics();
        let gd = e.graphics_device();

        let pfsstack = PerFrameStagingResourceStack::init(&g);

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
        let dset_render_offset = dpool.alloc(&[&dsl_u0])?[0];
        
        transfer_batch.add_mirroring_buffer(&buffer_upload, &buffer, 0, bp.total_size() as _);
        dsu_batch.write(dset_render_offset, 0,
            br::DescriptorUpdateInfo::UniformBuffer(vec![(buffer.native_ptr(), rs.render_offset_ub .. bp.total_size())]));
        return Ok(MainResources { stack: rs, buffer, dsl_u0, _dpool: dpool, dset_render_offset, pfsstack });
    }
    fn update_pfsbuffer<F: FnMut(&br::MappedMemoryRange)>(&self, updater: F) -> br::Result<()> {
        self.pfsstack.buffer.guard_map(self.pfsstack.total_size, updater)
    }
}
