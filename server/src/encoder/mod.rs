use std::os::raw::c_int;

use bytes::{buf::Writer, BytesMut};
use image::{ImageBuffer, Rgb};
use openh264::formats::YUVBuffer;

use self::openh264_enc::LVOpenH264Encoder;

#[cfg(feature = "nvidia-hwenc")]
pub mod nvidia;

pub mod openh264_enc;

pub fn default_encoder(
    width: u32,
    height: u32,
    bitrate: u32,
    fps: f32,
) -> Result<Box<dyn LVEncoder>, Box<dyn std::error::Error>> {
    let enc = LVOpenH264Encoder::new(width, height, bitrate, fps)?;

    #[cfg(feature = "nvidia-hwenc")]
    let enc = nvidia::LVNvidiaEncoder::new(width, height, bitrate, fps)?;

    Ok(Box::new(enc))
}

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

    fn bitrate(&self) -> u32;
    fn set_bitrate(&mut self, new_bitrate: u32) -> Result<(), Box<dyn std::error::Error>>;
}
