use super::{BufferBundle, BufferError, LoadedImageError, PipelineBundle, Vec2Int};
use core::mem::ManuallyDrop;
use gfx_hal::{
    adapter::{Adapter, MemoryTypeId, PhysicalDevice},
    buffer,
    device::Device,
    format::{Aspects, Format},
    image::Offset,
    image::{Layout, SubresourceRange, Usage},
    memory::{Properties, Requirements},
    pool::CommandPool,
    pso::PipelineStage,
    pso::{Descriptor, DescriptorSetWrite},
    Backend, Capability, CommandQueue, Supports, Transfer,
};
use std::{marker::PhantomData, ops::Deref};

pub struct LoadedImage<B: Backend> {
    pub image: ManuallyDrop<B::Image>,
    pub requirements: Requirements,
    pub memory: ManuallyDrop<B::Memory>,
    pub image_view: ManuallyDrop<B::ImageView>,
    pub sampler: ManuallyDrop<B::Sampler>,
    pub descriptor_set: ManuallyDrop<B::DescriptorSet>,
    pub phantom: PhantomData<B::Device>,
}

impl<B: Backend> LoadedImage<B> {
    pub fn allocate_and_create<C: Capability + Supports<Transfer>>(
        adapter: &Adapter<B>,
        device: &B::Device,
        command_pool: &mut CommandPool<B, C>,
        command_queue: &mut CommandQueue<B, C>,
        pipeline_bundle: &mut PipelineBundle<B>,
        img: &[u8],
        width: usize,
        height: usize,
        filter: gfx_hal::image::Filter,
    ) -> Result<Self, failure::Error> {
        unsafe {
            // Make the Image Object!
            let mut image_object = device
                .create_image(
                    gfx_hal::image::Kind::D2(width as u32, height as u32, 1, 1),
                    1,
                    Format::Rgba8Srgb,
                    gfx_hal::image::Tiling::Optimal,
                    Usage::TRANSFER_DST | Usage::SAMPLED,
                    gfx_hal::image::ViewCapabilities::empty(),
                )
                .map_err(|e| LoadedImageError::CreateImage(e))?;

            //  Allocate the memory and bind it
            let requirements = device.get_image_requirements(&image_object);
            let memory_type_id = adapter
                .physical_device
                .memory_properties()
                .memory_types
                .iter()
                .enumerate()
                .find(|&(id, memory_type)| {
                    requirements.type_mask & (1 << id) != 0
                        && memory_type.properties.contains(Properties::DEVICE_LOCAL)
                })
                .map(|(id, _)| MemoryTypeId(id))
                .ok_or(BufferError::MemoryId)?;

            let memory = device
                .allocate_memory(memory_type_id, requirements.size)
                .map_err(|e| BufferError::Allocate(e))?;

            device
                .bind_image_memory(&memory, 0, &mut image_object)
                .map_err(|e| BufferError::Bind(e))?;

            //  Create image view and sampler
            let image_view = device
                .create_image_view(
                    &image_object,
                    gfx_hal::image::ViewKind::D2,
                    Format::Rgba8Srgb,
                    gfx_hal::format::Swizzle::NO,
                    SubresourceRange {
                        aspects: Aspects::COLOR,
                        levels: 0..1,
                        layers: 0..1,
                    },
                )
                .map_err(|e| LoadedImageError::ImageView(e))?;

            let sampler = device
                .create_sampler(gfx_hal::image::SamplerInfo::new(
                    filter,
                    gfx_hal::image::WrapMode::Clamp,
                ))
                .map_err(|e| LoadedImageError::Sampler(e))?;

            let descriptor_set = pipeline_bundle.allocate_descriptor_set()?;

            //  Create our image and do some final tweaking to it!
            let texture = {
                let mut texture = Self {
                    image: manual_new!(image_object),
                    requirements,
                    memory: manual_new!(memory),
                    image_view: manual_new!(image_view),
                    sampler: manual_new!(sampler),
                    descriptor_set: manual_new!(descriptor_set),
                    phantom: PhantomData,
                };

                // Create a staging bundle of our passed in Data
                // and upload it into the image object
                texture.edit_image(
                    width as u32,
                    height as u32,
                    Vec2Int::new(0, 0),
                    img,
                    adapter,
                    device,
                    command_pool,
                    command_queue,
                )?;

                // Write that fucker: Write the descriptors into the descriptor set
                device.write_descriptor_sets(vec![
                    DescriptorSetWrite {
                        set: texture.descriptor_set.deref(),
                        binding: 0,
                        array_offset: 0,
                        descriptors: Some(Descriptor::Image(
                            texture.image_view.deref(),
                            Layout::ShaderReadOnlyOptimal,
                        )),
                    },
                    DescriptorSetWrite {
                        set: texture.descriptor_set.deref(),
                        binding: 1,
                        array_offset: 0,
                        descriptors: Some(Descriptor::Sampler(texture.sampler.deref())),
                    },
                ]);

                texture
            };

            Ok(texture)
        }
    }

    pub fn edit_image<C: Capability + Supports<Transfer>>(
        &mut self,
        width: u32,
        height: u32,
        offset: Vec2Int,
        data: &[u8],
        adapter: &Adapter<B>,
        device: &B::Device,
        command_pool: &mut CommandPool<B, C>,
        command_queue: &mut CommandQueue<B, C>,
    ) -> Result<(), failure::Error> {
        unsafe {
            // allocate texture
            let (staging_bundle, buffer_width) = LoadedImage::create_staging_buffer(
                adapter,
                device,
                data,
                width as usize,
                height as usize,
            )?;

            // edit the texture with the appropriate offset
            LoadedImage::load_staging_buffer_into_image_object(
                &*self.image,
                &staging_bundle,
                buffer_width,
                width,
                height,
                Offset {
                    x: offset.x,
                    y: offset.y,
                    z: 0,
                },
                device,
                command_pool,
                command_queue,
            )?;

            staging_bundle.manually_drop(device);

            // donzo!
            Ok(())
        }
    }

    unsafe fn create_staging_buffer(
        adapter: &Adapter<B>,
        device: &B::Device,
        img: &[u8],
        width: usize,
        height: usize,
    ) -> Result<(BufferBundle<B>, u32), failure::Error> {
        //  Memory garbanzo
        let limits = adapter.physical_device.limits();
        let row_alignment_mask = limits.optimal_buffer_copy_pitch_alignment as u32 - 1;

        let row_size = std::mem::size_of::<u32>() * width;
        let row_pitch = ((row_size as u32 + row_alignment_mask) & !row_alignment_mask) as usize;
        debug_assert!(row_pitch as usize >= row_size);

        let required_bytes = (row_pitch * height) as u64;
        let staging_bundle = BufferBundle::new(
            &adapter,
            device,
            required_bytes,
            buffer::Usage::TRANSFER_SRC,
            false,
        )?;
        //  Use a mapping writer to put the image data into the buffer
        let mut writer = device
            .acquire_mapping_writer::<u8>(
                &staging_bundle.memory,
                0..staging_bundle.requirements.size,
            )
            .map_err(|e| LoadedImageError::AcquireMappingWriter(e))?;

        for y in 0..height {
            let index = y * row_size..(y + 1) * row_size;
            let row_start = &(*img)[index];
            let dest_base = y * row_pitch;
            writer[dest_base..dest_base + row_start.len()].copy_from_slice(row_start);
        }

        device
            .release_mapping_writer(writer)
            .map_err(|e| LoadedImageError::ReleaseMappingWriter(e))?;

        Ok((
            staging_bundle,
            (row_pitch / std::mem::size_of::<u32>()) as u32,
        ))
    }

    unsafe fn load_staging_buffer_into_image_object<C: Capability + Supports<Transfer>>(
        image_object: &B::Image,
        staging_bundle: &BufferBundle<B>,
        buffer_width: u32,
        image_width: u32,
        image_height: u32,
        image_offset: Offset,
        device: &B::Device,
        command_pool: &mut CommandPool<B, C>,
        command_queue: &mut CommandQueue<B, C>,
    ) -> Result<(), failure::Error> {
        let mut cmd_buffer = command_pool.acquire_command_buffer::<gfx_hal::command::OneShot>();
        cmd_buffer.begin();

        //  Use a pipeline barrier to transition the image from empty/undefined
        //  to TRANSFER_WRITE/TransferDstOptimal
        let image_barrier = gfx_hal::memory::Barrier::Image {
            states: (gfx_hal::image::Access::empty(), Layout::Undefined)
                ..(
                    gfx_hal::image::Access::TRANSFER_WRITE,
                    Layout::TransferDstOptimal,
                ),
            target: image_object,
            families: None,
            range: SubresourceRange {
                aspects: Aspects::COLOR,
                levels: 0..1,
                layers: 0..1,
            },
        };
        cmd_buffer.pipeline_barrier(
            PipelineStage::TOP_OF_PIPE..PipelineStage::TRANSFER,
            gfx_hal::memory::Dependencies::empty(),
            &[image_barrier],
        );

        //  COPY THE BUFFER!
        cmd_buffer.copy_buffer_to_image(
            &staging_bundle.buffer,
            &image_object,
            Layout::TransferDstOptimal,
            &[gfx_hal::command::BufferImageCopy {
                buffer_offset: 0,
                buffer_width,
                buffer_height: image_height,
                image_layers: gfx_hal::image::SubresourceLayers {
                    aspects: Aspects::COLOR,
                    level: 0,
                    layers: 0..1,
                },
                image_offset: gfx_hal::image::Offset { x: 0, y: 0, z: 0 },
                image_extent: gfx_hal::image::Extent {
                    width: image_width,
                    height: image_height,
                    depth: 1,
                },
            }],
        );

        //  Use pipeline barrier to transition the image back to SHADER_READ
        //   and ShaderReadOnlyOptimal layout
        let image_barrier = gfx_hal::memory::Barrier::Image {
            states: (
                gfx_hal::image::Access::TRANSFER_WRITE,
                Layout::TransferDstOptimal,
            )
                ..(
                    gfx_hal::image::Access::SHADER_READ,
                    Layout::ShaderReadOnlyOptimal,
                ),
            target: image_object,
            families: None,
            range: SubresourceRange {
                aspects: Aspects::COLOR,
                levels: 0..1,
                layers: 0..1,
            },
        };
        cmd_buffer.pipeline_barrier(
            PipelineStage::TRANSFER..PipelineStage::FRAGMENT_SHADER,
            gfx_hal::memory::Dependencies::empty(),
            &[image_barrier],
        );

        //  Aaand we're done!
        cmd_buffer.finish();

        let upload_fence = device
            .create_fence(false)
            .map_err(|e| LoadedImageError::UploadFence(e))?;

        // Submit it!
        command_queue.submit_without_semaphores(Some(&cmd_buffer), Some(&upload_fence));

        device
            .wait_for_fence(&upload_fence, core::u64::MAX)
            .map_err(|e| LoadedImageError::WaitForFence(e))?;
        device.destroy_fence(upload_fence);

        //  11. Free our cmd_buffer!
        command_pool.free(Some(cmd_buffer));

        Ok(())
    }

    pub unsafe fn manually_drop(&self, device: &B::Device) {
        use core::ptr::read;
        device.destroy_sampler(ManuallyDrop::into_inner(read(&self.sampler)));
        device.destroy_image(manual_drop!(self.image));
        device.destroy_image_view(manual_drop!(self.image_view));
        device.free_memory(manual_drop!(self.memory));
    }
}
