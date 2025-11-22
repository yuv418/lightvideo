use std::{fs::File, io::Write};

use log::debug;

use super::LVVideoDecoder;

const FILENAME: &'static str = "test.h264";

/// Outputs a video to a file. Useful for debugging.
pub struct LVFileDecoder {
    file: File,
}

impl LVVideoDecoder for LVFileDecoder {
    fn new(
        _src_format: dcv_color_primitives::ImageFormat,
        _dst_format: dcv_color_primitives::ImageFormat,
        _double_buffer: std::sync::Arc<crate::double_buffer::DoubleBuffer>,
    ) -> Result<Self, Box<dyn std::error::Error>>
    where
        Self: Sized,
    {
        Ok(Self {
            file: File::create(FILENAME)?,
        })
    }

    fn decode(&mut self, _timestamp: u64, packet: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        self.file.write_all(packet)?;
        self.file.flush()?;

        debug!("wrote {} bytes to file", packet.len());

        Ok(())
    }
}
