use gfx_hal::device::{OomOrDeviceLost, OutOfMemory};

#[allow(unused_macros)]
macro_rules! quick_from {
    ($our_type:ty, $our_member:expr, $target_type:ty) => {
        impl From<$target_type> for $our_type {
            fn from(error: $target_type) -> Self {
                $our_member(error)
            }
        }
    };
}

use gfx_hal::buffer::CreationError;
#[derive(Debug, Fail)]
pub enum BufferBundleError {
    Creation(#[cause] CreationError),
}

impl std::fmt::Display for BufferBundleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let write_this = match self {
            BufferBundleError::Creation(e) => format!("Buffer creation error! => {}", e),
        };

        write!(f, "{}", write_this)
    }
}

#[derive(Debug, Fail)]
pub enum BufferError {
    MemoryId,
    Allocate(#[cause] gfx_hal::device::AllocationError),
    Bind(#[cause] gfx_hal::device::BindError),
}

impl std::fmt::Display for BufferError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let write_this = match self {
            BufferError::MemoryId => format!("MemoryID Error"),
            BufferError::Allocate(e) => format!("Buffer allocation error! => {}", e),
            BufferError::Bind(e) => format!("Buffer binding error! => {}", e),
        };

        write!(f, "{}", write_this)
    }
}

#[derive(Debug, Fail)]
pub enum LoadedImageError {
    AcquireMappingWriter(#[cause] gfx_hal::mapping::Error),
    ReleaseMappingWriter(#[cause] OutOfMemory),
    CreateImage(#[cause] gfx_hal::image::CreationError),
    ImageView(#[cause] gfx_hal::image::ViewError),
    Sampler(#[cause] gfx_hal::device::AllocationError),
    UploadFence(#[cause] OutOfMemory),
    WaitForFence(#[cause] OomOrDeviceLost),
}

impl std::fmt::Display for LoadedImageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let write_this = match self {
            LoadedImageError::AcquireMappingWriter(e) => format!(
                "Couldn't acquire a mapping writer to the staging buffer! => {}",
                e
            ),
            LoadedImageError::ReleaseMappingWriter(e) => format!(
                "Couldn't release the mapping writer to the staging buffer! => {}",
                e
            ),
            LoadedImageError::CreateImage(e) => format!("Couldn't create the image! => {}", e),
            LoadedImageError::ImageView(e) => format!("Couldn't create the image view! => {}", e),
            LoadedImageError::Sampler(e) => format!("Couldn't create the sampler! => {}", e),
            LoadedImageError::UploadFence(e) => {
                format!("Couldn't create the upload fence! => {}", e)
            }
            LoadedImageError::WaitForFence(e) => format!("Couldn't wait for the fence! => {}", e),
        };

        write!(f, "{}", write_this)
    }
}

#[derive(Debug, Fail)]
pub enum MemoryWritingError {
    #[fail(
        display = "Couldn't acquire a mapping writer to the staging buffer! => {}",
        _0
    )]
    AcquireMappingWriter(#[cause] gfx_hal::mapping::Error),
    #[fail(
        display = "Couldn't release the mapping writer to the staging buffer! => {}",
        _0
    )]
    ReleaseMappingWriter(#[cause] OutOfMemory),
}

quick_from!(
    MemoryWritingError,
    MemoryWritingError::AcquireMappingWriter,
    gfx_hal::mapping::Error
);

quick_from!(
    MemoryWritingError,
    MemoryWritingError::ReleaseMappingWriter,
    OutOfMemory
);
