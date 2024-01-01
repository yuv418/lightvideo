use bytes::{BufMut, Bytes, BytesMut};
use dcv_color_primitives::{convert_image, get_buffers_size, ColorSpace, ImageFormat};
use log::{debug, error, info, warn};
use openh264::{
    decoder::{DecodedYUV, Decoder, DecoderConfig},
    formats::YUVSource,
};
use rtp::{codecs::h264::H264Packet, packet::Packet, packetizer::Depacketizer};
use std::{net::UdpSocket, time::Instant};
use webrtc_util::Unmarshal;

pub fn decode(addr: &str) -> Result<(), Box<dyn std::error::Error>> {
    let sock = UdpSocket::bind(addr)?;

    let mut buf = [0; 1200];
    let mut pkt = H264Packet::default();
    let mut decoder = Decoder::with_config(DecoderConfig::new().debug(true))?;
    let mut buffer = BytesMut::new();
    let mut h264_data: Option<DecodedYUV> = None;
    let mut rgba_buffer: Option<Vec<u8>> = None;

    let mut width: u32 = 0;
    let mut height: u32 = 0;

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
        let time = Instant::now();
        let (amt, src) = sock.recv_from(&mut buf)?;
        debug!("recv received {} bytes from {}", amt, src);
        // TODO don't copy
        let mut bytes = Bytes::copy_from_slice(&buf[..amt]);
        // turn into packet
        let packet = Packet::unmarshal(&mut bytes)?;

        let is_partition_head = pkt.is_partition_head(&packet.payload);
        debug!("is partition head {}", is_partition_head);
        if is_partition_head {
            // Decode and clear buffer
            if !buffer.is_empty() {
                match decoder.decode(&buffer) {
                    Ok(yuv) => {
                        h264_data = yuv;
                        if let Some(ref yuv_data) = h264_data {
                            // Set up target buffer/data for calls to YUV->RGBA conversion
                            if let None = rgba_buffer {
                                let strides_yuv = yuv_data.strides_yuv();
                                width = strides_yuv.0 as u32;
                                height = yuv_data.height() as u32;
                                rgba_buffer = Some(vec![0; (4 * width * height) as usize]);
                            }
                            debug!("data width: {}, height: {}", width, height);

                            let mut src_sizes = [0usize; 3];
                            get_buffers_size(width, height, &src_format, None, &mut src_sizes)?;

                            let y = &yuv_data.y()[0..]; //src_sizes[0] + 1];
                            let u = &yuv_data.u()[0..]; //src_sizes[1] + 1];
                            let v = &yuv_data.v()[0..]; //src_sizes[2] + 1];

                            debug!(
                                        "converting image... dest buf size is {}, src_sizes is {:#?}, ysize usize vsize: [{}, {}, {}], strides from class are {:?}",
                                        rgba_buffer.as_mut().unwrap().len(),
                                        src_sizes, y.len(), u.len(), v.len(),
                                        yuv_data.strides_yuv()
                                    );

                            match convert_image(
                                width,
                                height,
                                &src_format,
                                None,
                                &[y, u, v],
                                &dst_format,
                                None,
                                &mut [&mut *rgba_buffer.as_mut().unwrap()],
                            ) {
                                Ok(_) => {}
                                Err(e) => warn!("converting image failed with {:?}, continuing", e),
                            }

                            std::fs::write("frame", &rgba_buffer.as_mut().unwrap())?;

                            // Convert YUV to Rgba8Uint so it can be copied to wgpu buffer.
                        }
                        // debug!("h264_data {:?}", h264_data);
                    }
                    Err(e) => {
                        error!("Failed to decode pkt {:?}", e)
                    }
                }
                buffer.clear();
            } else {
                warn!("skipping decode empty packet");
            }
        }
        buffer.put(pkt.depacketize(&packet.payload)?);

        // debug!("packet {:#?}", packet);

        info!("decode elapsed {:.4?}", time.elapsed());
    }
}
