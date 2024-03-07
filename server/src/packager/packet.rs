// A packet is a unit of data that is sent over the network.
// When we receive a RTP packet, we split it into CHUNK_SIZE-byte chunks.
//
// Note the CHUNK_SIZE is given as ceil(PKT_SIZE // EC_RATIO_REGULAR_PACKETS) rounded up to the nearest multiple of 64.
//
// We then use the error-correcting code library to generate exactly EC_RATIO_RECOVERY_PACKETS packets,

use bytes::Bytes;
use lazy_static::lazy_static;
use reed_solomon_simd::ReedSolomonEncoder;
use std::{net::UdpSocket, ops::Index, slice::SliceIndex};

use super::MTU_SIZE;

// TODO: Don't we want a packet size?

const EC_RATIO_RECOVERY_PACKETS: usize = 1;
const EC_RATIO_REGULAR_PACKETS: usize = 3;

const SIMD_PACKET_SIZE: usize = ((MTU_SIZE + 63) / 64) * 64;

pub struct LVErasureManager {
    enc: ReedSolomonEncoder,
    current_block_id: u32,
    current_fragment_index: u32,
}

impl LVErasureManager {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            enc: ReedSolomonEncoder::new(
                EC_RATIO_REGULAR_PACKETS,
                EC_RATIO_RECOVERY_PACKETS,
                SIMD_PACKET_SIZE,
            )?,
            current_block_id: 0,
            current_fragment_index: 0,
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
        payload: &[u8],
        recovery_pkt: bool,
    ) -> Result<usize, Box<dyn std::error::Error>> {
        let pk = LVPacket {
            block_id: self.current_block_id,
            fragment_index: self.current_fragment_index,
            min_fragment_size: EC_RATIO_REGULAR_PACKETS,
            recovery_pkt,
            payload_len: payload.len(),
            payload,
        };

        let pk_pay = unsafe {
            ::core::slice::from_raw_parts(
                (&pk as *const LVPacket) as *const u8,
                ::core::mem::size_of::<LVPacket>(),
            )
        };

        Ok(socket.send_to(pk_pay, target_addr)?)
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LVPacket<'a> {
    // Every block will have a unique error correcting ID.
    // This will allow us to know which packets go with which blocks.
    block_id: u32,
    // The minimum number of fragments that are required in this fragment sequence
    // to decode the full thing
    min_fragment_size: usize,
    // Allows us to determine if recovery packet or not, which is required for decoding
    recovery_pkt: bool,
    // Required for the decoding as well --- used to determine the # of the recovery
    // or regular packet (I would imagine that this is for polynomial interpolation or something)
    fragment_index: u32,
    // Given length of data
    payload_len: usize,
    // Actual data
    payload: &'a [u8],
}
