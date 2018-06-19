use bedrock as br; use br::traits::*;
use super::*;
use std::ops::Deref;
use std::mem::size_of;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferContent { Vertex(usize), Index(usize), Uniform(usize) }
impl BufferContent {
    fn usage(&self, src: br::BufferUsage) -> br::BufferUsage {
        match *self {
            BufferContent::Vertex(_) => src.vertex_buffer(),
            BufferContent::Index(_) => src.index_buffer(),
            BufferContent::Uniform(_) => src.uniform_buffer()
        }
    }
    fn alignment(&self, a: &br::PhysicalDevice) -> usize {
        match *self {
            BufferContent::Uniform(_) => a.properties().limits.minUniformBufferOffsetAlignment as _,
            _ => 1
        }
    }
    fn size(&self) -> usize {
        match *self { BufferContent::Vertex(v) | BufferContent::Index(v) | BufferContent::Uniform(v) => v }
    }

    /// Generic Shorthands
    pub fn vertex<T>() -> Self { BufferContent::Vertex(size_of::<T>()) }
    pub fn index<T>()  -> Self { BufferContent::Index(size_of::<T>()) }
    pub fn uniform<T>() -> Self { BufferContent::Uniform(size_of::<T>()) }
}
pub fn align2(v: usize, a: usize) -> usize { (v + (a - 1)) & !(a - 1) }
pub struct BufferPrealloc<'g> { g: &'g Graphics, usage: br::BufferUsage, offsets: Vec<usize>, total: usize }
impl<'g> BufferPrealloc<'g> {
    pub fn new(g: &'g Graphics) -> Self {
        BufferPrealloc { g, usage: br::BufferUsage(0), offsets: Vec::new(), total: 0 }
    }
    pub fn build(&self) -> br::Result<br::Buffer> {
        let obj = br::BufferDesc::new(self.total, self.usage).create(&self.g.device)?;
        return Ok(obj);
    }

    pub fn add(&mut self, content: BufferContent) -> usize {
        self.usage = content.usage(self.usage);
        let offs = align2(self.total, content.alignment(&self.g.adapter));
        self.total = offs + content.size();
        self.offsets.push(offs);
        return offs;
    }
    pub fn total_size(&self) -> usize { self.total }
}

pub struct MemoryBadget<'g> { g: &'g Graphics }
impl<'g> MemoryBadget<'g> {
    pub fn new(g: &'g Graphics) -> Self {
        MemoryBadget { g }
    }
    pub fn alloc_with_buffer(self, buffer: br::Buffer) -> br::Result<Buffer> {
        let breq = buffer.requirements();
        let mem: Rc<_> = br::DeviceMemory::allocate(&self.g.device, breq.size as _,
            self.g.memory_type_index_for(br::MemoryPropertyFlags::DEVICE_LOCAL).expect("No Device-Local memory"))?.into();
        return Buffer::bound(buffer, &mem, 0);
    }
    pub fn alloc_with_buffer_host_visible(self, buffer: br::Buffer) -> br::Result<Buffer> {
        let breq = buffer.requirements();
        let mem: Rc<_> = br::DeviceMemory::allocate(&self.g.device, breq.size as _,
            self.g.memory_type_index_for(br::MemoryPropertyFlags::HOST_VISIBLE).expect("No Host-Visible memory"))?.into();
        return Buffer::bound(buffer, &mem, 0);
    }
}

use std::mem::ManuallyDrop;
pub struct AutocloseMappedMemoryRange<'m>(&'m br::DeviceMemory, ManuallyDrop<br::MappedMemoryRange<'m>>);
impl<'m> Deref for AutocloseMappedMemoryRange<'m> {
    type Target = br::MappedMemoryRange<'m>; fn deref(&self) -> &Self::Target { &self.1 }
}
impl<'m> Drop for AutocloseMappedMemoryRange<'m> {
    fn drop(&mut self) {
        unsafe {
            ManuallyDrop::drop(&mut self.1);
            self.0.unmap();
        }
    }
}

#[derive(Clone)]
pub struct Buffer(Rc<br::Buffer>, Rc<br::DeviceMemory>, usize);
#[derive(Clone)]
pub struct Image(Rc<br::Image>, Rc<br::DeviceMemory>);
impl Buffer {
    pub fn bound(b: br::Buffer, mem: &Rc<br::DeviceMemory>, offset: usize) -> br::Result<Self> {
        b.bind(mem, offset).map(|_| Buffer(b.into(), mem.clone(), offset))
    }

    pub fn map(&self, size: usize) -> br::Result<br::MappedMemoryRange> {
        self.1.map(self.2 .. self.2 + size)
    }
    pub unsafe fn unmap(&self) { self.1.unmap() }
    pub fn guard_map<F: FnOnce(&br::MappedMemoryRange)>(&self, size: usize, f: F) -> br::Result<()> {
        f(&AutocloseMappedMemoryRange(&self.1, ManuallyDrop::new(self.map(size)?))); return Ok(());
    }
}
impl Image {
    pub fn bound(r: br::Image, mem: &Rc<br::DeviceMemory>, offset: usize) -> br::Result<Self> {
        r.bind(mem, offset).map(|_| Image(r.into(), mem.clone()))
    }
}
impl Deref for Buffer {
    type Target = br::Buffer; fn deref(&self) -> &br::Buffer { &self.0 }
}
impl Deref for Image {
    type Target = br::Image; fn deref(&self) -> &br::Image { &self.0 }
}
impl br::VkHandle for Buffer {
    type Handle = <br::Buffer as br::VkHandle>::Handle; fn native_ptr(&self) -> Self::Handle { self.0.native_ptr() }
}
impl br::VkHandle for Image {
    type Handle = <br::Image as br::VkHandle>::Handle; fn native_ptr(&self) -> Self::Handle { self.0.native_ptr() }
}
