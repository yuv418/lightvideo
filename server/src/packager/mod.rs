use std::{collections::VecDeque, fs::File, io::Write, net::UdpSocket, time::Instant};

use bytes::{buf::Writer, BufMut, Bytes, BytesMut};
use dcv_color_primitives::{convert_image, get_buffers_size, ColorSpace, ImageFormat};
use image::{ImageBuffer, Rgb};
use log::{debug, trace};
use openh264::formats::{YUVBuffer, YUVSource};
use rand::Rng;
use rtp::{
    codecs::h264::H264Payloader,
    header::Header,
    packet::Packet,
    packetizer::{Packetizer, Payloader},
    sequence::new_random_sequencer,
};
use statistics::{
    collector::LVStatisticsCollector,
    statistics::{LVDataPoint, LVDataType},
};

use crate::encoder::LVEncoder;

use self::packet::LVErasureManager;

pub mod packet;

const MTU_SIZE: usize = 1200;
const SAMPLE_RATE: u32 = 90000;

// TODO update the error handling

// Encode -> RTP Encapsulation -> Encrypt (skip for now) -> Error Correct (skip for now)
pub struct LVPackager {
    encoder: Box<dyn LVEncoder>,
    h264_bitstream_writer: Writer<BytesMut>,
    yuv_buffer: YUVBuffer,

    // TODO: Can we minimize the number of heap allocations with this?
    rtp_queue: VecDeque<Packet>,
    packetizer: Box<dyn Packetizer>,
    erasure_manager: LVErasureManager,
    file: File,
    fps: u32,
}

//
impl LVPackager {
    pub fn new(encoder: Box<dyn LVEncoder>, fps: u32) -> Result<Self, Box<dyn std::error::Error>> {
        let width = encoder.width() as usize;
        let height = encoder.height() as usize;
        let mut rand = rand::thread_rng();

        LVStatisticsCollector::register_data("server_packetization", LVDataType::TimeSeries);
        LVStatisticsCollector::register_data("server_queuing", LVDataType::TimeSeries);

        Ok(Self {
            encoder,
            // TODO: Default?       ?
            h264_bitstream_writer: BytesMut::new().writer(),
            rtp_queue: VecDeque::new(),
            yuv_buffer: YUVBuffer::new(width, height),
            packetizer: Box::new(rtp::packetizer::new_packetizer(
                MTU_SIZE,
                96,
                rand.gen_range(0..u32::MAX),
                Box::new(H264Payloader::default()),
                Box::new(new_random_sequencer()),
                SAMPLE_RATE,
            )),
            file: File::create("cap.h264")?,
            fps,
            erasure_manager: LVErasureManager::new()?,
        })
    }

    // Encode frame and add to RTP queue
    pub fn process_frame(
        &mut self,
        buffer: ImageBuffer<Rgb<u8>, Vec<u8>>,
        timestamp: u64,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let pre_enc = Instant::now();
        // Convert RGBA8 to YUV420
        self.encoder.convert_frame(buffer, &mut self.yuv_buffer);
        debug!("convert image sequence is {:.4?}", pre_enc.elapsed());

        let pre_enc = Instant::now();
        let bit_stream = self.encoder.encode_frame(
            &self.yuv_buffer,
            timestamp,
            &mut self.h264_bitstream_writer,
        )?;

        debug!(
            "bitstream buffer len after write is {}",
            self.h264_bitstream_writer.get_ref().len(),
        );

        // Extract RTP and put in queue from the bytes
        // The split will be dropped at the end of this function, so when we clear the bitstream writer and write to it later, it will use the whole buffer.
        let pre_enc = Instant::now();
        let unpacketized_payload: Bytes = Bytes::from(self.h264_bitstream_writer.get_mut().split());

        // write this for debugging purposes
        // self.file.write(&unpacketized_payload);

        debug!("unpacketized_payload len is {}", unpacketized_payload.len());

        let payloads = self
            .packetizer
            .packetize(&unpacketized_payload, SAMPLE_RATE / self.fps)?;

        LVStatisticsCollector::update_data(
            "server_packetization",
            LVDataPoint::TimeElapsed(pre_enc.elapsed()),
        );

        debug!("packetization: {:.4?}", pre_enc.elapsed());

        let pre_enc = Instant::now();
        let mut packet_count = 0;
        for payload in payloads {
            // Marshal into RTP.
            trace!("packet payload data: {:?}", &payload.payload.as_ref());
            trace!(
                "packet payload data len {}",
                &payload.payload.as_ref().len()
            );
            self.rtp_queue.push_front(payload);
            packet_count += 1;
        }
        debug!("wrote {} RTP packets into queue", packet_count);
        LVStatisticsCollector::update_data(
            "server_queuing",
            LVDataPoint::TimeElapsed(pre_enc.elapsed()),
        );

        Ok(())
    }

    pub fn send_next_pkt(
        &mut self,
        socket: &UdpSocket,
        target_addr: &str,
    ) -> Result<usize, Box<dyn std::error::Error>> {
        if let Some(pkt) = self.rtp_queue.pop_back() {
            return self
                .erasure_manager
                .send_lv_packet(socket, target_addr, &pkt.payload, false);
        } else {
            Ok(0)
        }
    }

    // Get the next RTP packet to send over the network
    pub fn has_rtp(&mut self) -> bool {
        !self.rtp_queue.is_empty()
    }

    // pub fn encrypt();
    // pub fn error_correct();
}
