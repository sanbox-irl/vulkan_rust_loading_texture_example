use super::{BufferBundleError, BufferError};
use core::mem::ManuallyDrop;
use gfx_hal::{
    adapter::{Adapter, MemoryTypeId, PhysicalDevice},
    buffer,
    device::Device,
    memory::{Properties, Requirements},
    Backend,
};
use std::{marker::PhantomData, mem};

pub struct BufferBundle<B: Backend> {
    pub buffer: ManuallyDrop<B::Buffer>,
    pub requirements: Requirements,
    pub mapped: Option<*mut u8>,
    pub memory: ManuallyDrop<B::Memory>,
    pub phantom: PhantomData<B::Device>,
}

impl<B: Backend> BufferBundle<B> {
    pub fn new(
        adapter: &Adapter<B>,
        device: &B::Device,
        size: u64,
        usage: buffer::Usage,
        map_it: bool,
    ) -> Result<Self, failure::Error> {
        unsafe {
            let mut buffer = device
                .create_buffer(size, usage)
                .map_err(|e| BufferBundleError::Creation(e))?;

            let requirements = device.get_buffer_requirements(&buffer);
            let memory_type_id = adapter
                .physical_device
                .memory_properties()
                .memory_types
                .iter()
                .enumerate()
                .find(|&(id, memory_type)| {
                    requirements.type_mask & (1 << id) != 0
                        && memory_type.properties.contains(Properties::CPU_VISIBLE)
                })
                .map(|(id, _)| MemoryTypeId(id))
                .ok_or(BufferError::MemoryId)?;
            let memory = device
                .allocate_memory(memory_type_id, requirements.size)
                .map_err(|e| BufferError::Allocate(e))?;

            device
                .bind_buffer_memory(&memory, 0, &mut buffer)
                .map_err(|e| BufferError::Bind(e))?;

            let mapped = if map_it {
                Some(device.map_memory(&memory, 0..requirements.size)?)
            } else {
                None
            };

            Ok(Self {
                buffer: manual_new!(buffer),
                requirements,
                memory: manual_new!(memory),
                phantom: PhantomData,
                mapped,
            })
        }
    }

    pub fn update_buffer<T>(&mut self, verts: &[T], vertex_offset: usize) {
        assert!(
            self.requirements.size >= (verts.len() * mem::size_of::<T>() + vertex_offset) as u64
        );

        // copy vertex data
        unsafe {
            if let Some(map) = &self.mapped {
                let dest = map.offset((vertex_offset * mem::size_of::<T>()) as isize);

                let src = &verts[0];
                std::ptr::copy_nonoverlapping(src, dest as *mut T, verts.len());
            }
        }
    }

    #[allow(dead_code)]
    pub fn has_room(&self, size: u64) -> bool {
        self.requirements.size >= size
    }

    pub unsafe fn manually_drop(&self, device: &B::Device) {
        use core::ptr::read;
        device.destroy_buffer(manual_drop!(self.buffer));
        device.free_memory(manual_drop!(self.memory));
    }

    pub unsafe fn flush(&self, device: &B::Device) -> Result<(), failure::Error> {
        device.flush_mapped_memory_ranges(&[(&*self.memory, ..)])?;
        Ok(())
    }
}
