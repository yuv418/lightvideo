use std::mem::size_of;

use bytes::BufMut;
use log::trace;

pub const MTU_SIZE: usize = 1200;
pub const EC_RATIO_RECOVERY_PACKETS: u32 = 2;
pub const EC_RATIO_REGULAR_PACKETS: u32 = 4;

pub const SIMD_PACKET_SIZE: u32 =
    ((MTU_SIZE as u32 - LVErasureInformation::no_bytes() as u32 + 63) / 64) * 64;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LVErasureInformation {
    // Every block will have a unique error correcting ID.
    // This will allow us to know which packets go with which blocks.
    pub block_id: u32,
    // The minimum number of fragments that are required in this fragment sequence
    // to decode the full thing
    pub min_fragment_size: u32,
    // Allows us to determine if recovery packet or not, which is required for decoding
    pub recovery_pkt: bool,
    // Required for the decoding as well --- used to determine the # of the recovery
    // or regular packet (I would imagine that this is for polynomial interpolation or something)
    pub fragment_index: u32,
    // For the recovery packet: store the packet sizes for the other regular packets
    // so we know how much to truncate after using the SIMD decoder.
    //
    // We can store this as a u16 because the largest packet size over UDP can be stored as a u16 value.
    pub pkt_sizes: [u16; EC_RATIO_REGULAR_PACKETS as usize],
}

impl LVErasureInformation {
    pub const fn no_bytes() -> usize {
        3 * size_of::<u32>()
            + size_of::<bool>()
            + size_of::<[u16; EC_RATIO_REGULAR_PACKETS as usize]>()
    }

    // TODO
    // maybe don't use big endian. it's prolly faster.
    pub fn to_bytes(self, buf: &mut [u8]) {
        let mut i = 0;
        for byt in self.block_id.to_be_bytes() {
            buf[i] = byt;
            i += 1;
        }

        for byt in self.min_fragment_size.to_be_bytes() {
            buf[i] = byt;
            i += 1;
        }

        buf[i] = self.recovery_pkt as u8;
        i += 1;

        for byt in self.fragment_index.to_be_bytes() {
            buf[i] = byt;
            i += 1;
        }

        for pksz in self.pkt_sizes {
            for byt in pksz.to_be_bytes() {
                buf[i] = byt;
                i += 1;
            }
        }
        trace!("now buf is {:?}", buf);
    }

    pub fn from_bytes(buf: &[u8]) -> Self {
        // not a huge fan of this.
        let mut pkt_sizes: [u16; EC_RATIO_REGULAR_PACKETS as usize] =
            [0; EC_RATIO_REGULAR_PACKETS as usize];
        for i in 0..EC_RATIO_REGULAR_PACKETS {
            pkt_sizes[i as usize] = u16::from_be_bytes(
                buf[(13 + (i * 2) as usize)..(15 + (i * 2) as usize)]
                    .try_into()
                    .unwrap(),
            );
        }

        Self {
            block_id: u32::from_be_bytes(buf[0..4].try_into().unwrap()),
            min_fragment_size: u32::from_be_bytes(buf[4..8].try_into().unwrap()),
            recovery_pkt: buf[8] != 0,
            fragment_index: u32::from_be_bytes(buf[9..13].try_into().unwrap()),
            pkt_sizes,
        }
    }
}
