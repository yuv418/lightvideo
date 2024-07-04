use std::sync::Arc;

use dcv_color_primitives::ImageFormat;

use crate::double_buffer::DoubleBuffer;

pub mod openh264;
pub mod vaapi;

pub trait LVVideoDecoder {
    fn new(
        width: u32,
        height: u32,
        src_format: ImageFormat,
        dst_format: ImageFormat,
        double_buffer: Arc<DoubleBuffer>,
    ) -> Result<Self, Box<dyn std::error::Error>>
    where
        Self: Sized;

    fn decode(&mut self, timestamp: u64, packet: &[u8]) -> Result<(), Box<dyn std::error::Error>>;
}
