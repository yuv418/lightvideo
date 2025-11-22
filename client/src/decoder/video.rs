use bytes::{Buf, BufMut, Bytes, BytesMut};
use dcv_color_primitives::{ColorSpace, ImageFormat, convert_image, get_buffers_size};
use log::{debug, error, info, trace, warn};
use openh264::{
    decoder::{Decoder, DecoderConfig},
    formats::YUVSource,
};
use parking_lot::{Mutex, RwLock};
use reed_solomon_simd::ReedSolomonDecoder;
use rtp::{codecs::h264::H264Packet, packet::Packet, packetizer::Depacketizer};
use statistics::{
    collector::LVStatisticsCollector,
    statistics::{LVDataPoint, LVDataType},
};
use std::{collections::VecDeque, os::fd::RawFd, sync::Arc, thread, time::Instant};
use thingbuf::mpsc::blocking::Receiver;
use webrtc_util::Unmarshal;

use net::{
    feedback_packet::{self, LVAck, LVFeedbackPacket},
    packet::{
        EC_RATIO_RECOVERY_PACKETS, EC_RATIO_REGULAR_PACKETS, LVErasureInformation, SIMD_PACKET_SIZE,
    },
};

use crate::decoder::{
    network::LVPacketHolder,
    video_decoder::{file::LVFileDecoder, openh264::LVOpenH264Decoder, vaapi::LVVAAPIDecoder},
};
use crate::double_buffer::DoubleBuffer;

use nix::ioctl_read_bad;
use nix::libc::TIOCOUTQ;

use super::video_decoder::LVVideoDecoder;

ioctl_read_bad!(tiocoutq, TIOCOUTQ, u32);

pub struct LVDecoder {
    width: u32,
    height: u32,
    buffer: Vec<u8>,
    decoder: Box<dyn LVVideoDecoder>,
    pkt: H264Packet,
}

impl LVDecoder {
    // TODO Might be an Arc
    pub fn new(decoder: Box<dyn LVVideoDecoder>) -> Self {
        Self {
            width: 0,
            height: 0,
            buffer: Vec::new(),
            decoder,
            pkt: H264Packet::default(),
        }
    }

    pub fn run(
        double_buffer: Arc<DoubleBuffer>,
        packet_recv: Receiver<LVPacketHolder>,
        feedback_pkt: Arc<Mutex<(LVAck, LVFeedbackPacket)>>,
        udp_fd: Arc<RwLock<Option<RawFd>>>,
    ) {
        thread::Builder::new()
            .name("decoder_thread".to_string())
            .spawn(move || {
                if let Err(e) = Self::decode_loop(double_buffer, packet_recv, feedback_pkt, udp_fd)
                {
                    error!("decode loop failed with error {:?}", e);
                } else {
                    info!("decode receive loop exited.");
                }
            });
    }

    pub fn bytes_in_send_queue(
        udp_fd: Arc<RwLock<Option<RawFd>>>,
    ) -> Result<Option<u32>, Box<dyn std::error::Error>> {
        // info!("udp fd set to {:?}", self.udp_fd);
        match *udp_fd.read() {
            Some(fd) => unsafe {
                let mut data = 0;
                tiocoutq(fd, &mut data)?;
                Ok(Some(data))
            },
            None => Ok(None),
        }
    }

    pub fn depacketize_decode(
        &mut self,
        packet: &Packet,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let time = Instant::now();

        debug!(
            "depacketize: packet seq num is {}",
            packet.header.sequence_number
        );

        let is_partition_head = self.pkt.is_partition_head(&packet.payload);
        debug!("is partition head {}", is_partition_head);
        if is_partition_head {
            // Decode and clear buffer
            if !self.buffer.is_empty() {
                // call decode handler here.
                trace!("buffer is {:?}", self.buffer);
                self.decoder
                    .decode(packet.header.timestamp as u64, &self.buffer)?
            } else {
                debug!("skipping decode empty packet");
            }
            // if there's an empty packet and a boundary we need to clear the buffer. In both cases the buffer must be cleared.
            self.buffer.clear();
        }
        let depacketized_payload = self.pkt.depacketize(&packet.payload)?;
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
        self.buffer.extend_from_slice(&depacketized_payload);

        LVStatisticsCollector::update_data(
            "client_decode_packet",
            LVDataPoint::TimeElapsed(time.elapsed()),
        );

        Ok(())
    }

    pub fn decode_loop(
        double_buffer: Arc<DoubleBuffer>,
        packet_recv: Receiver<LVPacketHolder>,
        feedback_pkt: Arc<Mutex<(LVAck, LVFeedbackPacket)>>,
        udp_fd: Arc<RwLock<Option<RawFd>>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        debug!("starting thread for decode");

        LVStatisticsCollector::register_data("client_packets_out_of_order", LVDataType::Aggregate);
        LVStatisticsCollector::register_data("client_decode_packet", LVDataType::TimeSeries);
        LVStatisticsCollector::register_data("client_failed_decode_packets", LVDataType::Aggregate);

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
        // let mut decoder = Decoder::with_config(DecoderConfig::new().debug(true))?;
        // let mut decoder = LVOpenH264Decoder::new(src_format, dst_format, double_buffer)?;
        let decoder = LVVAAPIDecoder::new(src_format, dst_format, double_buffer)?;
        let mut video_dec = Self::new(Box::new(decoder));

        let mut width: u32 = 0;
        let mut height: u32 = 0;

        let mut lvheader_prev_fragment_index: u32 = EC_RATIO_REGULAR_PACKETS - 1;

        let mut rs_decoder = ReedSolomonDecoder::new(
            EC_RATIO_REGULAR_PACKETS as usize,
            EC_RATIO_RECOVERY_PACKETS as usize,
            SIMD_PACKET_SIZE as usize,
        )?;

        let mut rs_fragment_buffer = vec![0; SIMD_PACKET_SIZE as usize];
        let mut rs_sendq = vec![Default::default(); EC_RATIO_REGULAR_PACKETS as usize];
        let mut rs_total_packets = 0;
        let mut rs_pkt_sizes = [0; EC_RATIO_REGULAR_PACKETS as usize];
        let mut rs_inorder_packets = 0;
        let mut rs_recovery_packets = 0;
        let mut rs_oorder_packets = 0;
        let mut block_id = 0;

        // TODO offload the feedback to statistics module
        let mut total_blocks = 0;
        let mut out_of_order_blocks = 0;
        let mut total_packets = 0;
        let mut lost_packets = 0;
        let mut ecc_decoder_failures = 0;

        let mut average_qocc = 0;
        let mut average_qocc_iterations = 0;

        // TODO what happened to re-ordering RTP packets?
        loop {
            // TODO don't copy. We slice the buffer so it only uses the part of the buffer that was written to by the socket receive.
            let data = packet_recv.recv_ref();

            if let None = data {
                error!("the network push buffer has closed");
                return Err(Box::new(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "no more network data",
                )));
            }
            // horrible rust coding practices 101
            let data_ext = data.unwrap();
            debug!(
                "data_ext length is {}",
                data_ext.payload[0..data_ext.amt].len()
            );
            debug!("data_ext is {:?}", &data_ext.payload[0..data_ext.amt]);

            // extract the data into the RTP payload and the lv erasure header

            let lvheader = LVErasureInformation::from_bytes(&data_ext.payload[0..data_ext.amt]);
            let mut rtp_data = &data_ext.payload[LVErasureInformation::no_bytes()..data_ext.amt];

            total_packets += 1;

            // new block
            if lvheader.block_id != block_id {
                total_blocks += 1;
                // recovery
                if rs_total_packets < EC_RATIO_RECOVERY_PACKETS + EC_RATIO_REGULAR_PACKETS
                    && rs_total_packets - rs_recovery_packets != EC_RATIO_REGULAR_PACKETS
                {
                    debug!(
                        "RECOVERY: decoded {} packets in order, {} total packets in block {}, beginning error recovery",
                        rs_inorder_packets, rs_total_packets, block_id
                    );

                    out_of_order_blocks += 1;
                    match rs_decoder.decode() {
                        Ok(data) => {
                            for (k, mut v) in data.restored_original_iter() {
                                info!("RECOVERY: recovered packet {}", k);

                                // TODO. this assumes that recovery occurs ON the recovery packet, which is bad.
                                let mut slc = &v[..rs_pkt_sizes[k] as usize];
                                debug!("slice with rtp is {:?}", slc);
                                debug!("full slice is {:?}", v);
                                rs_sendq[k] = Packet::unmarshal(&mut slc)?;
                                debug!(
                                    "RECOVERY: recovered packet header is {:?}",
                                    rs_sendq[k].header
                                );
                            }

                            // send all packets in rs_sendq[rs_inorder_packets..] to depacketizer
                            debug!(
                                "length of sendq sliced for inorder packets is {}",
                                rs_sendq[rs_inorder_packets..].len()
                            );
                            for (i, pkt_inorder) in
                                rs_sendq[rs_inorder_packets..].iter().enumerate()
                            {
                                debug!(
                                    "RECOVERY: sending packet {} to decoder",
                                    rs_inorder_packets + i
                                );
                                video_dec.depacketize_decode(pkt_inorder)?;
                            }
                        }
                        Err(e) => {
                            ecc_decoder_failures += 1;
                            warn!("recovery failed with {:?}", e);
                        }
                    }

                    lvheader_prev_fragment_index = EC_RATIO_REGULAR_PACKETS - 1;
                }

                debug!("new block, resetting decoder and total packets");

                rs_inorder_packets = 0;
                rs_recovery_packets = 0;
                rs_oorder_packets = 0;
                rs_total_packets = 0;

                rs_decoder.reset(
                    EC_RATIO_REGULAR_PACKETS as usize,
                    EC_RATIO_RECOVERY_PACKETS as usize,
                    SIMD_PACKET_SIZE as usize,
                )?;

                block_id = lvheader.block_id;
            }

            // prepare for missing packets by putting every received packet into the decoder.

            rs_fragment_buffer[0..rtp_data.len()].clone_from_slice(rtp_data);
            rs_fragment_buffer[rtp_data.len()..].fill(0);

            debug!("Received lvheader {:?}", lvheader);
            debug!("Received lvdata {:?}", rtp_data);
            debug!("lvdata remaining {}", rtp_data.remaining());
            debug!("recved data from socket thread");

            if lvheader.recovery_pkt {
                rs_decoder
                    .add_recovery_shard(lvheader.fragment_index as usize, &rs_fragment_buffer)?;

                debug!("Added recovery shard to decoder, continuin");

                rs_pkt_sizes = lvheader.pkt_sizes;

                rs_recovery_packets += 1;
                rs_total_packets += 1;

                continue;
            }

            rs_decoder.add_original_shard(lvheader.fragment_index as usize, &rs_fragment_buffer)?;
            rs_total_packets += 1;
            // turn into packet
            let packet = Packet::unmarshal(&mut rtp_data)?;

            debug!("packet timestamp {}", packet.header.timestamp);
            debug!("packet seqnum {}", packet.header.sequence_number);

            if rs_oorder_packets > 0 {
                debug!("adding packet to oorder packets");

                lost_packets += lvheader.fragment_index
                    - ((lvheader_prev_fragment_index + 1) % EC_RATIO_REGULAR_PACKETS);
                lvheader_prev_fragment_index = lvheader.fragment_index as u32;
                rs_sendq[lvheader.fragment_index as usize] = packet;
                rs_oorder_packets += 1;
                continue;
            }

            if (lvheader_prev_fragment_index + 1) % EC_RATIO_REGULAR_PACKETS
                != lvheader.fragment_index
            {
                debug!(
                    "packet out of order: current {} prev {}",
                    lvheader.fragment_index, lvheader_prev_fragment_index
                );
                // ERROR: line 347 here can crash
                lost_packets += lvheader.fragment_index
                    - ((lvheader_prev_fragment_index + 1) % EC_RATIO_REGULAR_PACKETS);

                // TODO this statistic is wrong
                LVStatisticsCollector::update_data(
                    "client_packets_out_of_order",
                    LVDataPoint::Increment,
                );

                // add to "queue"
                lvheader_prev_fragment_index = lvheader.fragment_index as u32;
                rs_oorder_packets += 1;
                rs_sendq[lvheader.fragment_index as usize] = packet;

                continue;
            }

            if rs_oorder_packets == 0 {
                rs_inorder_packets += 1;
            }

            lvheader_prev_fragment_index = lvheader.fragment_index as u32;
            video_dec.depacketize_decode(&packet)?;

            match Self::bytes_in_send_queue(udp_fd.clone()) {
                Ok(Some(d)) => {
                    average_qocc += d;
                    average_qocc_iterations += 1;
                }
                _ => {
                    warn!("udp fd none or err");
                }
            }

            match feedback_pkt.try_lock() {
                Some(mut pkt) => {
                    // Reset our variables
                    let reset = pkt.1.total_packets == 0;

                    debug!("total_blocks {}", total_blocks);
                    debug!("out_of_order_blocks {}", out_of_order_blocks);
                    debug!("total_packets {}", total_packets);
                    debug!("lost_packets {}", lost_packets);
                    debug!("ecc_decoder_failures {}", ecc_decoder_failures);

                    pkt.1.total_blocks = total_blocks;
                    pkt.1.out_of_order_blocks = out_of_order_blocks;
                    pkt.1.total_packets = total_packets;
                    pkt.1.lost_packets = lost_packets as u16;
                    pkt.1.ecc_decoder_failures = ecc_decoder_failures;
                    pkt.1.average_buffer_occupancy = average_qocc / average_qocc_iterations;

                    if reset {
                        total_blocks = 0;
                        out_of_order_blocks = 0;
                        total_packets = 0;
                        lost_packets = 0;
                        ecc_decoder_failures = 0;
                        average_qocc = 0;
                        average_qocc_iterations = 0;
                    }

                    pkt.0.rtp_seqno = packet.header.sequence_number;
                    pkt.0.send_ts = lvheader.send_timestamp;
                }
                None => {
                    warn!("Failed to lock feedback packet")
                }
            }
        }
    }
}
