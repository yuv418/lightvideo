use std::mem::size_of;

use serde::{Deserialize, Serialize};

pub const ACK_TYPE: u8 = 0;
pub const FEEDBACK_TYPE: u8 = 1;

const EMPTY_PKT: LVFeedbackPacket = LVFeedbackPacket {
    time_quantum: 0,
    total_blocks: 0,
    out_of_order_blocks: 0,
    total_packets: 0,
    ecc_decoder_failures: 0,
    average_buffer_occupancy: 0,
    lost_packets: 0,
};

#[repr(C)]
#[derive(bytemuck::NoUninit, bytemuck::AnyBitPattern, Clone, Copy, Default, Debug)]
pub struct LVFeedbackHeader {
    pub feedback_type: u8,
}

// Used for RTT calculation
#[repr(C, packed)]
#[derive(Serialize, Deserialize, bytemuck::NoUninit, Clone, Copy, Default, Debug)]
pub struct LVAck {
    // Sequence number for packet
    pub rtp_seqno: u16,
    // Time
    pub send_ts: u128,
}

impl LVAck {
    pub fn no_bytes() -> usize {
        size_of::<u128>() + size_of::<u16>()
    }
}

#[repr(C, packed)]
#[derive(
    Serialize, Deserialize, bytemuck::NoUninit, bytemuck::AnyBitPattern, Clone, Copy, Default, Debug,
)]
pub struct LVFeedbackPacket {
    // In ms, a u16 limits the max time quantum if we use ms
    // but ~25s max should be fine.
    pub time_quantum: u16,
    // Number of total blocks sent per quantum
    pub total_blocks: u16,
    // Total number of times we try to decode packets due to out-of-order sequences
    pub out_of_order_blocks: u16,
    // Number of lost packets
    pub total_packets: u16,
    // Number of lost packets
    pub lost_packets: u16,

    // Number of failures to decode in the ECC decoder. To find the
    // ratio of failures : attempted decodes, we can simply do self.out_of_order_blocks/self.ecc_decoder_failures
    pub ecc_decoder_failures: u16,

    // Average buffer occupancy over the last time quantum on the client
    pub average_buffer_occupancy: u32,
}

impl LVFeedbackPacket {
    pub fn no_bytes() -> usize {
        bytemuck::bytes_of(&EMPTY_PKT).len()
    }
}
