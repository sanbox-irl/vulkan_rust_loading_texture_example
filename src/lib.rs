#![warn(elided_lifetimes_in_paths)]
#![allow(unreachable_code)]

#[macro_use]
extern crate failure;

// Macros
macro_rules! manual_drop {
    ($this_val:expr) => {
        ManuallyDrop::into_inner(read(&$this_val))
    };
}

macro_rules! manual_new {
    ($this_val:ident) => {
        ManuallyDrop::new($this_val)
    };
}

mod buffer_bundle;
mod errors;
mod loaded_image;
mod pipeline_bundle;
mod utilities;

use buffer_bundle::*;
use errors::*;
use loaded_image::*;
use pipeline_bundle::PipelineBundle;
use utilities::*;

#[cfg(feature = "dx12")]
use gfx_backend_dx12 as back;
#[cfg(feature = "metal")]
use gfx_backend_metal as back;
#[cfg(feature = "vulkan")]
use gfx_backend_vulkan as back;

pub struct RendererComponent;

pub fn register_texture(
    renderer: &mut RendererComponent,
    image: &image::RgbaImage,
) -> Result<usize, failure::Error> {
    let texture = {
        let mut pipeline_bundle = unimplemented!();

        LoadedImage::allocate_and_create(
            unimplemented!(),
            unimplemented!(),
            unimplemented!(),
            unimplemented!(),
            &mut pipeline_bundle,
            &*image,
            image.width() as usize,
            image.height() as usize,
            gfx_hal::image::Filter::Nearest,
        )?
    };

    let ret = unimplemented!();
    /*
    // Add to some texture array here...
    renderer.textures.push(texture);
    */

    Ok(ret)
}
