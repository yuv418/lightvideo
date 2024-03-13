// A packet is a unit of data that is sent over the network.
// When we receive a RTP packet, we split it into CHUNK_SIZE-byte chunks.
//
// Note the CHUNK_SIZE is given as ceil(PKT_SIZE // EC_RATIO_REGULAR_PACKETS) rounded up to the nearest multiple of 64.
//
// We then use the error-correcting code library to generate exactly EC_RATIO_RECOVERY_PACKETS packets,

use bytes::{BufMut, Bytes, BytesMut};
use lazy_static::lazy_static;
use log::{debug, trace};
use reed_solomon_simd::ReedSolomonEncoder;
use rtp::packet::Packet;
use std::{net::UdpSocket, ops::Index, slice::SliceIndex, time::SystemTime};
use webrtc_util::{Marshal, MarshalSize};

use net::packet::LVErasureInformation;

use super::MTU_SIZE;

// TODO: Don't we want a packet size?

const EC_RATIO_RECOVERY_PACKETS: u32 = 1;
const EC_RATIO_REGULAR_PACKETS: u32 = 3;

const SIMD_PACKET_SIZE: u32 = ((MTU_SIZE as u32 + 63) / 64) * 64;

pub struct LVErasureManager {
    enc: ReedSolomonEncoder,
    current_block_id: u32,
    current_regular_fragment_index: u32,
    current_recovery_fragment_index: u32,
    pkt_data: BytesMut,
}

impl LVErasureManager {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            enc: ReedSolomonEncoder::new(
                EC_RATIO_REGULAR_PACKETS as usize,
                EC_RATIO_RECOVERY_PACKETS as usize,
                SIMD_PACKET_SIZE as usize,
            )?,
            current_block_id: 0,
            current_regular_fragment_index: 0,
            current_recovery_fragment_index: 0,
            pkt_data: BytesMut::zeroed(
                SIMD_PACKET_SIZE as usize + LVErasureInformation::no_bytes(),
            ),
        })
    }

    // Given an input packet,
    // return a pair, where the first packet is the payload given as an LVPacket and
    // the second packet is an Option<LVPacket> that contains recovery data
    // if the encoder gave us some.
    pub fn send_lv_packet(
        &mut self,
        socket: &UdpSocket,
        target_addr: &str,
        rtp: Packet,
        recovery_pkt: bool,
    ) -> Result<usize, Box<dyn std::error::Error>> {
        // self.enc.add_original_shard(payload)?;

        let pk = LVErasureInformation {
            block_id: self.current_block_id,
            fragment_index: self.current_regular_fragment_index,
            min_fragment_size: EC_RATIO_REGULAR_PACKETS,
            recovery_pkt,
        };

        trace!("lv erasure information {:?}", pk);

        // Every time we hit the end of the number of recovery packets, we increment the block id.
        self.current_regular_fragment_index =
            (self.current_regular_fragment_index + 1) % EC_RATIO_REGULAR_PACKETS;

        if self.current_regular_fragment_index == 0 {
            self.current_block_id += 1;
        }

        let marshal_size = rtp.marshal_size();

        trace!(
            "size of bytes of erasure information is {}",
            LVErasureInformation::no_bytes()
        );
        trace!(
            "should send {} bytes",
            LVErasureInformation::no_bytes() + marshal_size
        );

        // trace!("payload is {:?}", payload);

        pk.to_bytes(&mut self.pkt_data);
        rtp.marshal_to(
            &mut self.pkt_data[(LVErasureInformation::no_bytes())
                ..(LVErasureInformation::no_bytes() + marshal_size)],
        );

        let send_slice = &self.pkt_data[0..(LVErasureInformation::no_bytes() + marshal_size)];
        debug!("sent lv packet as {:?}", send_slice);

        Ok(socket.send_to(send_slice, target_addr)?)
    }
}
