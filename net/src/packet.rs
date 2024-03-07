#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LVPacket<'a> {
    // Every block will have a unique error correcting ID.
    // This will allow us to know which packets go with which blocks.
    pub block_id: u32,
    // The minimum number of fragments that are required in this fragment sequence
    // to decode the full thing
    pub min_fragment_size: usize,
    // Allows us to determine if recovery packet or not, which is required for decoding
    pub recovery_pkt: bool,
    // Required for the decoding as well --- used to determine the # of the recovery
    // or regular packet (I would imagine that this is for polynomial interpolation or something)
    pub fragment_index: u32,
    // Given length of data
    pub payload_len: usize,
    // Actual data
    pub payload: &'a [u8],
}
