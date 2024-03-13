use std::mem::size_of;

use bytes::BufMut;
use log::trace;

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
}

impl LVErasureInformation {
    pub const fn no_bytes() -> usize {
        3 * size_of::<u32>() + size_of::<bool>()
    }

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
        trace!("now buf is {:?}", buf);
    }

    pub fn from_bytes(buf: &[u8]) -> Self {
        Self {
            block_id: u32::from_be_bytes(buf[0..4].try_into().unwrap()),
            min_fragment_size: u32::from_be_bytes(buf[4..8].try_into().unwrap()),
            recovery_pkt: buf[9] != 0,
            fragment_index: u32::from_be_bytes(buf[9..13].try_into().unwrap()),
        }
    }
}
