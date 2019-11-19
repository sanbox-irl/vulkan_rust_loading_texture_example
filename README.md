# Loading a Texture with Vulkan

Hi there! I'm Jack and [I exist exclusively here](https://twitter.com/sanbox_irl). This document was originally published as a medium post. I'd link to it, but I'm comitting this before I publish it, so that's how it goes.

## Quick, Bring These

This walkdown (somewhere between a *walkthrough* and a *rundown*) is about how I handled loading textures to the GPU using Vulkan in my Rust game engine from scratch!

If you've never done any graphics or Rust before, have no fear, but also, don't read this article. If you've would like to get into Vulkan/game engine programming and like Rust, check out [this excellent guide](https://github.com/rust-tutorials/learn-gfx-hal) and then come back after you've drawn a textured quad. Otherwise, follow me!

## The Image Object and Its Destructor

Everything we need in an "image" we're going to slap into a single struct. We won't use all these fields here, but the reason to keep them bundled is simple -- we don't want Rust to automatically deconstruct any of them, so we need to haul them around. I call this "loaded image"...`LoadedImage`. It's also a badass name 😎.

```rs
pub struct LoadedImage<B: gfx_hal::Backend> {
    pub image: ManuallyDrop<B::Image>,
    pub requirements: Requirements,
    pub memory: ManuallyDrop<B::Memory>,
    pub image_view: ManuallyDrop<B::ImageView>,
    pub sampler: ManuallyDrop<B::Sampler>,
    pub descriptor_set: ManuallyDrop<B::DescriptorSet>,
    pub phantom: PhantomData<B::Device>,
}
```

First things first, let's be good C-citizens (when we're this `unsafe` in Rust, we're not far from just writing C) and add our destructor:

```rs
    pub unsafe fn manually_drop(&self, device: &B::Device) {
        use core::ptr::read;
        device.destroy_sampler(ManuallyDrop::into_inner(read(&self.sampler)));
        device.destroy_image(manual_drop!(self.image));
        device.destroy_image_view(manual_drop!(self.image_view));
        device.free_memory(manual_drop!(self.memory));
    }
```

It's difficult to force the compiler to force us to use the `manually_drop` method when we make an image -- normally that's a thing Rust handles easily, but we've basically "turned that off" by using `ManuallyDrop`, so I guess we'll just have to use our dumb brains to remember to do it (fun fact -- while writing this article, I forgot to include the section about dropping some memory, ironically showing why this kind of memory management can be error prone).

`manual_drop`, by the way, is a simple convencience macro because I got tired of writing this all the time:

```rs
macro_rules! manual_drop {
    ($this_val:expr) => {
        ManuallyDrop::into_inner(read(&$this_val))
    };
}
```

## Creating an Image

Okay, now we're onto the **good bits!** Let's walk through what actually making an image looks like.

### Parameters

```rs
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
    ) -> Result<LoadedImage, failure::Error> {
```

There's two things to point out here of note: `width` and `height`. These refer to the texel size of the image. If you've never heard the term `texel`, bless your heart, because it's a terrible word. A `texel` is to a texture like a `pixel` is to a...`picture`....which is what a `texture` is...oh no!

Okay, so what is a `texture`? For us, we're using a very simple definition (mipmaps complicate this!): a texture is a 2D grid of `colors`, and a color is 4 `u8`s in a row, forming an `RGBA` image.

Here's an example of a texture written out...

```txt
000 000 000 255     080 010 012 255     200 210 012 255
112 243 025 255     091 000 200 255     111 222 333 255
```

So when we say the `width` and `height` of a texture, we're really asking about *this grid*. That's going to matter in a minute when we get dynamic.

### Making the Actual Image Object

This whole section is largely boilerplate, but let's run through it quickly.

First, we say, "Hey, GPU, make me an image please" and it  says "sure, here ya go":

```rs
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
```

We'll also need to find the requirements for how much memory the GPU is going to need. This ultimatley is up to the GPU to tell us, since GPUs might pad memory differently, but it's going to be in the ballpark of `width * height * 4`, 4 being how many bytes are in one color.

We get that memory requirement, ask the GPU to allocate that memory, and then we bind that memory to our image object (I'm not sure what's going on with the term `bind`, but I assume we're essentially giving one part of our GPU a pointer to the memory in the GPU). That looks like this:

```rs
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
```

That's all fairly boilerplate stuff. Here's where things get a little fancier: making our image_view and our sampler. It's difficult for me here to get into too much detail, as these things really are bound to your `descriptor_sets` which come from the `DescriptorPool` you'll create in your `PipelineLayout`. For me, as a simple 2D man with a simple 2D game, it looks like this:

```rs
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
```

And **finally**, we create our `LoadedImage` like this:

```rs
let mut texture = Self {
    image: manual_new!(image_object),
    requirements,
    memory: manual_new!(memory),
    image_view: manual_new!(image_view),
    sampler: manual_new!(sampler),
    descriptor_set: manual_new!(descriptor_set),
    phantom: PhantomData,
};
```

## Editing the Image Object

Okay! So now we have a `LoadedImage`. You'll notice we bound it to a `mut texture` before we returned it out of its constructor, and that's because we're not done yet. It's time to actually edit the image so it looks like what we want!

To edit this image, or any image, we need to create a buffer, which we'll fill with our `u8s` to our heart's content, and then we need to put that buffer in our pipeline to send into our image!

### Create our Staging Buffer

First, we're going to need to do some **pointer funtime math**! Here's what we're going to need to do:

```rs
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
```

What's that `BufferBundle::new` function? It's **exactly** like how we made an image object, but just slightly tweaked to be about *buffers* instead of *images*.

```rs
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
```

The returned `BufferBundle` looks like this, just to keep it all out there:

```rs
pub struct BufferBundle<B: gfx_hal::Backend> {
    pub buffer: ManuallyDrop<B::Buffer>,
    pub requirements: Requirements,
    pub mapped: Option<*mut u8>,
    pub memory: ManuallyDrop<B::Memory>,
    pub phantom: PhantomData<B::Device>,
}
```

Now here's the real meat of the problem -- we need to write the stream of image data we have *to the buffer*. This code is dense, so read over it a few times for clarification. For me, grabbing a piece of paper and doing it myself gave me a good feel, but basically, we're trying to convert a flat array to grid, copying each row at a time to the GPU.

```rs
//  Use a mapping writer to put the image data into the buffer
let mut writer = device
    .acquire_mapping_writer::<u8>(&staging_bundle.memory, 0..staging_bundle.requirements.size)
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
```

And with that, our `staging_buffer` is good to go! We need one last piece of data, and that's simple:

```rs
let buffer_width = row_pitch / std::mem::size_of::<u32>()) as u32;
```

I have this all bound in as as a function which returns a tuple of `(BufferBundle, u32)`, which is good enough. See the linked repo for more.

## Uploading the Buffer to the Image Object

Okay, so when you talk to the GPU in Vulkan you need two things:

1. The data you want to operate on in some sort of buffer. We just made ours when we made our "staging buffer" and prepared it with our image data.
2. A "command buffer" which is just another buffer that you upload to the GPU which has references to the buffer(s) you want to operate on.

To make our command buffer, we ask the GPU for one out of our `CommandPool`, which we make in our `Pipeline` creation (see the learn gfx_hal tutorials above for that!):

```rs
let mut cmd_buffer = command_pool.acquire_command_buffer::<gfx_hal::command::OneShot>();
cmd_buffer.begin();
```

Our image is in some `undefined` state right now (as in, I personally don't know what state it's in!), so we'll need to transfer it to a state where we can write to it. We do this with a **barrier**, and we create on like this:

```rs
//  Use a pipeline barrier to transition the image from empty/undefined
//  to TRANSFER_WRITE/TransferDstOptimal
let image_barrier = gfx_hal::memory::Barrier::Image {
    states: (gfx_hal::image::Access::empty(), Layout::Undefined)
        ..(gfx_hal::image::Access::TRANSFER_WRITE, Layout::TransferDstOptimal),
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
```

Next, we do what we *actually* want to be doing here, which is copying the buffer over! We do it like this:

```rs
cmd_buffer.copy_buffer_to_image(
    &staging_bundle.buffer,
    &image_object,
    Layout::TransferDstOptimal, // notice how the destination layout is the same here...
    &[gfx_hal::command::BufferImageCopy {
        buffer_offset: 0,
        buffer_width, // this is the buffer width from before
        buffer_height: image_height, // just the image height. So for 32x64 image, this is 64.
        image_layers: gfx_hal::image::SubresourceLayers {
            aspects: Aspects::COLOR,
            level: 0,
            layers: 0..1,
        },
        image_offset: gfx_hal::image::Offset {
            x: 0,
            y: 0,
            z: 0,
        },
        image_extent: gfx_hal::image::Extent {
            width: image_width, // just the image width, not the buffer width. So for a 32x64 image, this is 32
            height: image_height, // just the image height. So for 32x64 image, this is 64.
            depth: 1,
        },
    }],
);
```

Important note here: if you instead want to make a dynamic texture (which I may cover in a brief addendum in the future), where you edit a *part* of a texture *after* creating it, you can easily do that by making `width` and `height` only a section of the image, and then specify some `offset` into the image. You can also just re-edit the *entire* texture at once, but that's awfully wasteful!

Now, we need to transition our `image` *back* to being in the state of `SHADER_READ` and the layout of `ShaderReadOnlyOptimal`. We do that with...you guessed it, another barrier, like so:

```rs
//  Use pipeline barrier to transition the image back to SHADER_READ
//   and ShaderReadOnlyOptimal layout
let image_barrier = gfx_hal::memory::Barrier::Image {
    states: (gfx_hal::image::Access::TRANSFER_WRITE, Layout::TransferDstOptimal)
        ..(gfx_hal::image::Access::SHADER_READ, Layout::ShaderReadOnlyOptimal),
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
```

And then we finish up `cmd_buffer`. Before we submit it, we make a fence. For those who don't know, a `fence`, in Vulkan speak, is similar to a semaphore, but a fence is used between the CPU and the GPU and a semaphore is used between different parts of the CPU. (They're actually implemented very differently -- a semaphore is a software thing and a fence is a hardware thing, but it's not terrible important). All the `barriers` we've been making are actually `fences`!

```rs
//  Aaand we're done!
cmd_buffer.finish();

let upload_fence = device
    .create_fence(false)
    .map_err(|e| LoadedImageError::UploadFence(e))?;

// Submit it!
command_queue.submit_without_semaphores(Some(&cmd_buffer), Some(&upload_fence));
```

As **always**, we need to do our cleanup here too! First, we wait on our fence to make sure that our command buffer has finished being uploaded to the GPU, and then we free it and destroy the fence:

```rs
// Cleanup
device
    .wait_for_fence(&upload_fence, core::u64::MAX)
    .map_err(|e| LoadedImageError::WaitForFence(e))?;
device.destroy_fence(upload_fence);

command_pool.free(Some(cmd_buffer));
```

And, with that, we are done!

## Final Example from the Caller's Perspective

Let's take a step back and let's see how this code look in our wider program.

I make a wrapper function called `register_texture` which requires my `RendererComponent`, which is where my `pipeline`, `command_pool`, `command_queue`, `adapter`, and `device` live, and an `RgbaImage`. This is a struct provided by the [image crate](https://crates.io/crates/image). 

The function looks like this:

```rs
pub fn register_texture(renderer: &mut RC, image: &RgbaImage) -> Result<usize, Error> {
    let texture = {
        let mut pipeline_bundle = &mut renderer.pipeline_bundles[RC::STANDARD_PIPELINE];

        LoadedImage::allocate_and_create(
            &renderer.adapter,
            &renderer.device,
            &mut renderer.command_pool,
            &mut renderer.queue_group.queues[0],
            &mut pipeline_bundle,
            &*image,
            image.width() as usize,
            image.height() as usize,
            gfx_hal::image::Filter::Nearest,
        )?
    };

    let ret = renderer.textures.len();
    renderer.textures.push(texture);

    Ok(ret)
}
```

That looks pretty good to me!

---

Thanks so much for joining me on this walkdown through loading a texture in Vulkan using `gfx_hal`. I hope this has been useful to you!

You can always find me [here where I exist perpetually](https://twitter.com/sanbox_irl).
