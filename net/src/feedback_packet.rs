use std::mem::size_of;

#[repr(C)]
#[derive(bytemuck::NoUninit, Clone, Copy)]
pub struct LVFeedbackPacket {
    // In ms, a u16 limits the max time quantum if we use ms
    // but ~25ms should be fine.
    pub time_quantum: u16,
    // Number of lost packets
    pub lost_packets: u16,
    // Total number of times we try to decode packets due to out-of-order sequences
    pub out_of_order_blocks: u16,
    // Number of failures to decode in the ECC decoder. To find the
    // ratio of failures : attempted decodes, we can simply do self.out_of_order_blocks/self.ecc_decoder_failures
    pub ecc_decoder_failures: u16,
}

impl LVFeedbackPacket {
    pub const fn no_bytes() -> usize {
        4 * size_of::<u16>()
    }
}
