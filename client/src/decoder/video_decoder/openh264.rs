use std::sync::Arc;

use dcv_color_primitives::{convert_image, get_buffers_size, ColorSpace, ImageFormat};
use log::{debug, error, warn};
use openh264::{
    decoder::{DecodedYUV, Decoder, DecoderConfig},
    formats::YUVSource,
};
use statistics::{collector::LVStatisticsCollector, statistics::LVDataPoint};

use crate::double_buffer::DoubleBuffer;

use super::LVVideoDecoder;

pub struct LVOpenH264Decoder {
    width: u32,
    height: u32,
    src_format: ImageFormat,
    dst_format: ImageFormat,
    decoder: Decoder,
    double_buffer: Arc<DoubleBuffer>,
}

impl LVVideoDecoder for LVOpenH264Decoder {
    fn new(
        width: u32,
        height: u32,
        src_format: ImageFormat,
        dst_format: ImageFormat,
        double_buffer: Arc<DoubleBuffer>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            width,
            height,
            src_format,
            dst_format,
            decoder: Decoder::with_config(DecoderConfig::new().debug(true))?,
            double_buffer,
        })
    }

    fn decode(&mut self, _timestamp: u64, packet: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        match self.decoder.decode(packet) {
            Ok(yuv) => {
                Ok(if let Some(ref yuv_data) = yuv {
                    // Set up target buffer/data for calls to YUV->RGBA conversion
                    debug!("data width: {}, height: {}", self.width, self.height);

                    if self.double_buffer.uninitialized() {
                        let strides_yuv = yuv_data.strides_yuv();
                        self.width = strides_yuv.0 as u32;
                        self.height = yuv_data.height() as u32;
                        self.double_buffer.initialize(
                            (4 * self.width * self.height) as usize,
                            self.width as usize,
                            self.height as usize,
                        );
                    }

                    // New scope so rgba_buffer is dropped before swap
                    {
                        let mut rgba_buffer = self.double_buffer.back().unwrap();

                        let mut src_sizes = [0usize; 3];
                        get_buffers_size(
                            self.width,
                            self.height,
                            &self.src_format,
                            None,
                            &mut src_sizes,
                        )?;

                        let y = &yuv_data.y()[0..]; //src_sizes[0] + 1];
                        let u = &yuv_data.u()[0..]; //src_sizes[1] + 1];
                        let v = &yuv_data.v()[0..]; //src_sizes[2] + 1];

                        debug!(
                                        "converting image... dest buf size is {}, src_sizes is {:#?}, ysize usize vsize: [{}, {}, {}], strides from class are {:?}",
                                        rgba_buffer.as_mut().unwrap().buffer.len(),
                                        src_sizes, y.len(), u.len(), v.len(),
                                        yuv_data.strides_yuv()
                                    );

                        // Convert YUV to Rgba8Uint so it can be copied to wgpu buffer.
                        match convert_image(
                            self.width,
                            self.height,
                            &self.src_format,
                            None,
                            &[y, u, v],
                            &self.dst_format,
                            None,
                            &mut [&mut *rgba_buffer.as_mut().unwrap().buffer],
                        ) {
                            Ok(_) => {}
                            Err(e) => {
                                warn!("converting image failed with {:?}, continuing", e)
                            }
                        }
                    }

                    // swap doublebuffer
                })
                // debug!("h264_data {:?}", h264_data);
            }
            Err(e) => {
                error!("Failed to decode pkt {}", e);
                if let Some(bt) = e.backtrace() {
                    error!("backtrace: {}", bt);
                }

                LVStatisticsCollector::update_data(
                    "client_failed_decode_packets",
                    LVDataPoint::Increment,
                );

                Ok(())
            }
        }
    }
}
