use bytes::{BufMut, Bytes, BytesMut};
use dcv_color_primitives::{convert_image, get_buffers_size, ColorSpace, ImageFormat};
use log::{debug, error, info, trace, warn};
use openh264::{
    decoder::{Decoder, DecoderConfig},
    formats::YUVSource,
};
use rtp::{codecs::h264::H264Packet, packet::Packet, packetizer::Depacketizer};
use statistics::{
    collector::LVStatisticsCollector,
    statistics::{LVDataPoint, LVDataType},
};
use std::{sync::Arc, thread, time::Instant};
use thingbuf::mpsc::blocking::Receiver;
use webrtc_util::Unmarshal;

use crate::decoder::network::LVPacket;
use crate::double_buffer::DoubleBuffer;

pub struct LVDecoder {}

impl LVDecoder {
    // TODO Might be an Arc
    pub fn new() -> Self {
        Self {}
    }

    pub fn run(&self, double_buffer: Arc<DoubleBuffer>, packet_recv: Receiver<LVPacket>) {
        thread::Builder::new()
            .name("decoder_thread".to_string())
            .spawn(move || {
                if let Err(e) = Self::decode_loop(double_buffer, packet_recv) {
                    error!("decode loop failed with error {:?}", e);
                } else {
                    info!("decode receive loop exited.");
                }
            });
    }

    pub fn decode_loop(
        double_buffer: Arc<DoubleBuffer>,
        packet_recv: Receiver<LVPacket>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        debug!("starting thread for decode");

        LVStatisticsCollector::register_data("client_decode_packet", LVDataType::TimeSeries);

        let mut pkt = H264Packet::default();
        let mut decoder = Decoder::with_config(DecoderConfig::new().debug(true))?;
        let mut buffer = Vec::new();

        let mut width: u32 = 0;
        let mut height: u32 = 0;

        let mut rtp_prev_timestamp: u32 = 0;

        let src_format = ImageFormat {
            pixel_format: dcv_color_primitives::PixelFormat::I420,
            color_space: ColorSpace::Bt601FR,
            num_planes: 3,
        };
        let dst_format = ImageFormat {
            pixel_format: dcv_color_primitives::PixelFormat::Rgba,
            color_space: ColorSpace::Rgb,
            num_planes: 1,
        };

        // TODO what happened to re-ordering RTP packets?
        loop {
            // TODO don't copy. We slice the buffer so it only uses the part of the buffer that was written to by the socket receive.
            let time = Instant::now();
            let mut bytes = {
                let data = packet_recv.recv();
                if let Some(data) = data {
                    data.payload
                } else {
                    error!("the network push buffer has closed");
                    return Err(Box::new(std::io::Error::new(
                        std::io::ErrorKind::BrokenPipe,
                        "no more network data",
                    )));
                }
            };
            debug!("recved data from socket thread");
            // turn into packet
            let packet = Packet::unmarshal(&mut bytes)?;

            debug!("packet timestamp {}", packet.header.timestamp);

            if rtp_prev_timestamp > packet.header.timestamp {
                warn!(
                    "packet out of order: current {} prev {}",
                    packet.header.timestamp, rtp_prev_timestamp
                );
            }

            rtp_prev_timestamp = packet.header.timestamp;

            let is_partition_head = pkt.is_partition_head(&packet.payload);
            debug!("is partition head {}", is_partition_head);
            if is_partition_head {
                // Decode and clear buffer
                if !buffer.is_empty() {
                    match decoder.decode(&buffer) {
                        Ok(yuv) => {
                            if let Some(ref yuv_data) = yuv {
                                // Set up target buffer/data for calls to YUV->RGBA conversion
                                if double_buffer.uninitialized() {
                                    let strides_yuv = yuv_data.strides_yuv();
                                    width = strides_yuv.0 as u32;
                                    height = yuv_data.height() as u32;
                                    double_buffer.initialize(
                                        (4 * width * height) as usize,
                                        width as usize,
                                        height as usize,
                                    );
                                }
                                debug!("data width: {}, height: {}", width, height);

                                // New scope so rgba_buffer is dropped before swap
                                {
                                    let mut rgba_buffer = double_buffer.back().unwrap();

                                    let mut src_sizes = [0usize; 3];
                                    get_buffers_size(
                                        width,
                                        height,
                                        &src_format,
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
                                        width,
                                        height,
                                        &src_format,
                                        None,
                                        &[y, u, v],
                                        &dst_format,
                                        None,
                                        &mut [&mut *rgba_buffer.as_mut().unwrap().buffer],
                                    ) {
                                        Ok(_) => {}
                                        Err(e) => {
                                            warn!(
                                                "converting image failed with {:?}, continuing",
                                                e
                                            )
                                        }
                                    }
                                }

                                // swap doublebuffer
                                double_buffer.swap();
                            }
                            // debug!("h264_data {:?}", h264_data);
                        }
                        Err(e) => {
                            error!("Failed to decode pkt {}", e);
                            if let Some(bt) = e.backtrace() {
                                error!("backtrace: {}", bt);
                            }
                        }
                    }
                } else {
                    warn!("skipping decode empty packet");
                }
                // if there's an empty packet and a boundary we need to clear the buffer. In both cases the buffer must be cleared.
                buffer.clear();
            }
            let depacketized_payload = pkt.depacketize(&packet.payload)?;
            if depacketized_payload.is_empty() {
                trace!(
                    "depacketized payload is empty! payload is {:?}",
                    &packet.payload[..]
                );
            } else {
                trace!(
                    "depacketized payload is NOT empty {:?}",
                    &depacketized_payload[..]
                );

                trace!(
                    "payload for NONEMPTY depacketized is {:?}",
                    &packet.payload[..]
                );
            }
            buffer.extend_from_slice(&depacketized_payload);

            LVStatisticsCollector::update_data(
                "client_decode_packet",
                LVDataPoint::TimeElapsed(time.elapsed()),
            );
        }
    }
}
