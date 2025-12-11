use anyhow::anyhow;
use dcv_color_primitives::{convert_image, ImageFormat};

pub struct ImageFormatConverter {
    src_format: ImageFormat,
    dst_format: ImageFormat,
    width: u32,
    height: u32,
}

impl ImageFormatConverter {
    pub fn new(src_format: ImageFormat, dst_format: ImageFormat, width: u32, height: u32) -> Self {
        Self {
            src_format,
            dst_format,
            width,
            height,
        }
    }

    pub fn convert(
        &self,
        y: &[u8],
        u: &[u8],
        v: &[u8],
        dst_buffer: &mut [u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        // slicing isn't done here since the buffers may be separate for different programs.

        match convert_image(
            self.width,
            self.height,
            &self.src_format,
            None,
            &[y, u, v],
            &self.dst_format,
            None,
            &mut [dst_buffer],
        ) {
            Ok(_) => Ok(()),
            Err(e) => Err(anyhow!("converting image failed with {:?}, continuing", e).into()),
        }
    }
}
