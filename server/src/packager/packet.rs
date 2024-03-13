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

use net::packet::{
    LVErasureInformation, EC_RATIO_RECOVERY_PACKETS, EC_RATIO_REGULAR_PACKETS, SIMD_PACKET_SIZE,
};

// TODO: Don't we want a packet size?

pub struct LVErasureManager {
    enc: ReedSolomonEncoder,
    current_block_id: u32,
    current_regular_fragment_index: u32,
    current_recovery_fragment_index: u32,
    largest_sized_payload: usize,
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
            largest_sized_payload: 0,
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
    ) -> Result<usize, Box<dyn std::error::Error>> {
        let pk = LVErasureInformation {
            block_id: self.current_block_id,
            fragment_index: self.current_regular_fragment_index,
            min_fragment_size: EC_RATIO_REGULAR_PACKETS,
            recovery_pkt: false,
        };

        trace!("lv erasure information {:?}", pk);

        // Every time we hit the end of the number of recovery packets, we increment the block id.
        self.current_regular_fragment_index =
            (self.current_regular_fragment_index + 1) % EC_RATIO_REGULAR_PACKETS;

        if self.current_regular_fragment_index == 0 {
            debug!("obtaining recovery data from reed solomon code");

            self.current_recovery_fragment_index =
                (self.current_recovery_fragment_index + 1) % EC_RATIO_RECOVERY_PACKETS;

            let recovery_payload = self.enc.encode()?;
            let recovery_header = LVErasureInformation {
                block_id: self.current_block_id,
                fragment_index: self.current_recovery_fragment_index,
                min_fragment_size: EC_RATIO_REGULAR_PACKETS,
                recovery_pkt: true,
            };

            debug!("recovery header is {:?}", recovery_header);

            recovery_header.to_bytes(&mut self.pkt_data);

            for recovery_pkt in recovery_payload.recovery_iter() {
                let pkt_slice = &mut self.pkt_data[(LVErasureInformation::no_bytes())
                    ..(LVErasureInformation::no_bytes() + self.largest_sized_payload)];

                debug!("largest sized payload was {}", self.largest_sized_payload);
                debug!("recovery payload is {:?}", recovery_pkt);

                // RS recovery packet will not have payload larger than largest sized RTP packet,
                // so we can just slice the array as 0..self.largest_sized_payload

                pkt_slice.copy_from_slice(&recovery_pkt[0..self.largest_sized_payload]);
            }

            // send recovery packet over network

            let send_slice =
                &self.pkt_data[..(self.largest_sized_payload + LVErasureInformation::no_bytes())];

            debug!("send slice is {:?}", send_slice);
            let bytes = socket.send_to(send_slice, target_addr)?;
            debug!("send {} RECOVERY bytes to {}", bytes, target_addr);

            self.largest_sized_payload = 0;

            self.current_block_id += 1;
        }

        let marshal_size = rtp.marshal_size();
        if marshal_size > self.largest_sized_payload {
            self.largest_sized_payload = marshal_size;
        }

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

        {
            let mut rtp_slice = &mut self.pkt_data[(LVErasureInformation::no_bytes())
                ..(LVErasureInformation::no_bytes() + marshal_size)];
            rtp.marshal_to(&mut rtp_slice)?;

            // zero out the data that wasn't written to ):
            // TODO can we make this any more efficient?
            // because the trade off is if I don't do this,
            // we send more data which is useless,
            // but then if I do this it takes more time.
            self.pkt_data[(LVErasureInformation::no_bytes() + marshal_size)..].fill(0);

            self.enc
                .add_original_shard(&self.pkt_data[LVErasureInformation::no_bytes()..])?;
        }

        let send_slice = &self.pkt_data[0..(LVErasureInformation::no_bytes() + marshal_size)];
        debug!("sent lv packet as {:?}", send_slice);

        Ok(socket.send_to(send_slice, target_addr)?)
    }
}
