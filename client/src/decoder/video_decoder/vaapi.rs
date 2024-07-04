use std::{rc::Rc, sync::Arc};

use anyhow::anyhow;
use cros_codecs::{
    backend::vaapi::decoder::VaapiBackend,
    decoder::{
        stateless::{h264::H264, StatelessDecoder, StatelessVideoDecoder},
        DecodedHandle, DecoderEvent,
    },
    libva::Display,
};
use dcv_color_primitives::ImageFormat;
use log::debug;
use nix::libc::stack_t;
use openh264::decoder::DecodedYUV;

use crate::double_buffer::DoubleBuffer;

use super::LVVideoDecoder;

pub struct LVVAAPIDecoder {
    width: u32,
    height: u32,
    decoder: StatelessDecoder<H264, VaapiBackend<()>>,
    double_buffer: Arc<DoubleBuffer>,
}

impl LVVideoDecoder for LVVAAPIDecoder {
    fn new(
        width: u32,
        height: u32,
        src_format: ImageFormat,
        dst_format: ImageFormat,
        double_buffer: Arc<DoubleBuffer>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        match Display::open() {
            Some(disp) => {
                let decoder: StatelessDecoder<H264, VaapiBackend<()>> =
                    StatelessDecoder::<H264, VaapiBackend<()>>::new_vaapi(
                        disp,
                        cros_codecs::BlockingMode::Blocking,
                    )?;
                Ok(Self {
                    width,
                    height,
                    decoder,
                    double_buffer,
                })
            }
            None => Err(anyhow!("failed to open VA-API display").into()),
        }
    }

    fn decode(&mut self, timestamp: u64, packet: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        let amt_decoded = self.decoder.decode(timestamp, packet)?;
        let stream_info = self.decoder.stream_info().unwrap();
        debug!(
            "stream info is {:?} and amount decoded is {}",
            stream_info.format, amt_decoded
        );
        if self.double_buffer.uninitialized() {
            self.width = stream_info.display_resolution.width;
            self.height = stream_info.display_resolution.height;
            self.double_buffer.initialize(
                (4 * self.width * self.height) as usize,
                self.width as usize,
                self.height as usize,
            );
        }

        match self.decoder.next_event() {
            Some(ev) => match ev {
                DecoderEvent::FrameReady(frame) => {
                    let mut db_frame = self.double_buffer.back().unwrap();
                    let dyn_pic = frame.dyn_picture();
                    let mut mappable_handle = dyn_pic.dyn_mappable_handle()?;
                    // We need to check what kind of image format

                    mappable_handle.read(&mut db_frame.as_mut().unwrap().buffer)?;

                    Ok(())
                }
                DecoderEvent::FormatChanged(chg) => Ok(()),
            },
            None => Ok(()),
        }
    }
}
