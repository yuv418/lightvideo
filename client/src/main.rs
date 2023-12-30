use std::{net::UdpSocket, time::Instant};

use bytes::{BufMut, Bytes, BytesMut};
use flexi_logger::Logger;
use log::{debug, error, info, warn};
use openh264::{
    decoder::{DecodedYUV, Decoder, DecoderConfig},
    formats::YUVBuffer,
};
use rtp::{codecs::h264::H264Packet, packet::Packet, packetizer::Depacketizer};
use webrtc_util::Unmarshal;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    Logger::try_with_str("debug")?.start()?;

    // Bind value
    match std::env::args().nth(1).as_deref() {
        Some(addr) => {
            let sock = UdpSocket::bind(addr)?;

            let mut buf = [0; 1200];
            let mut pkt = H264Packet::default();
            let mut decoder = Decoder::with_config(DecoderConfig::new().debug(true))?;
            let mut buffer = BytesMut::new();
            let mut h264_data: Option<DecodedYUV> = None;

            loop {
                let time = Instant::now();
                let (amt, src) = sock.recv_from(&mut buf)?;
                debug!("recv received {} bytes from {}", amt, src);
                // TODO don't copy
                let mut bytes = Bytes::copy_from_slice(&buf);
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
                                debug!("h264_data {:?}", h264_data);
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
        None => println!("Usage: ./client addr"),
    }
    Ok(())
}
