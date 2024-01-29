use std::os::raw::c_int;

use ::openh264::formats::{YUVBuffer, YUVSource};
use bytes::{buf::Writer, BytesMut};

pub mod nvidia;
pub mod openh264;

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

    // TODO: Make our own version of YUVSource that maybe has an "into YUVSource" kind of thing.
    fn encode_frame(
        &mut self,
        buffer: &YUVBuffer,
        // Milliseconds from start.
        timestamp: u64,
        h264_buffer: &mut Writer<BytesMut>,
    ) -> Result<(), Box<dyn std::error::Error>>;
}
