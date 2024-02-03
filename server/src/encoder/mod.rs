use std::os::raw::c_int;

use bytes::{buf::Writer, BytesMut};
use image::{ImageBuffer, Rgb};
use openh264::formats::YUVBuffer;

pub mod nvidia;
pub mod openh264_enc;

pub trait LVEncoder {
    fn new(
        width: u32,
        height: u32,
        bitrate: u32,
        framerate: f32,
    ) -> Result<Self, Box<dyn std::error::Error>>
    where
        Self: Sized;

    fn width(&self) -> u32;
    fn height(&self) -> u32;

    // Convert the RGBA/whatever frame to something that the codec will understand
    fn convert_frame(
        &mut self,
        input_buffer: ImageBuffer<Rgb<u8>, Vec<u8>>,
        output_buffer: &mut YUVBuffer,
    ) -> Result<(), Box<dyn std::error::Error>>;

    // TODO: Make our own version of YUVSource that maybe has an "into YUVSource" kind of thing.
    fn encode_frame(
        &mut self,
        buffer: &YUVBuffer,
        // Milliseconds from start.
        timestamp: u64,
        h264_buffer: &mut Writer<BytesMut>,
    ) -> Result<(), Box<dyn std::error::Error>>;
}
