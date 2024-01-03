use std::{collections::VecDeque, time::Instant};

use bytes::{buf::Writer, BufMut, Bytes, BytesMut};
use dcv_color_primitives::{convert_image, get_buffers_size, ColorSpace, ImageFormat};
use image::{ImageBuffer, Rgb};
use log::debug;
use openh264::formats::{YUVBuffer, YUVSource};
use rand::Rng;
use rtp::{
    codecs::h264::H264Payloader,
    header::Header,
    packet::Packet,
    packetizer::{Packetizer, Payloader},
    sequence::new_random_sequencer,
};

use crate::encoder::LVEncoder;

const MTU_SIZE: usize = 1200;
const SAMPLE_RATE: u32 = 90000;

// TODO update the error handling

// Encode -> RTP Encapsulation -> Encrypt (skip for now) -> Error Correct (skip for now)
pub struct LVPackager {
    encoder: LVEncoder,
    h264_bitstream_writer: Writer<BytesMut>,
    yuv_buffer: YUVBuffer,

    // TODO: Can we minimize the number of heap allocations with this?
    rtp_queue: VecDeque<Packet>,
    packetizer: Box<dyn Packetizer>,
    fps: u32,
}

//
impl LVPackager {
    pub fn new(encoder: LVEncoder, fps: u32) -> Result<Self, Box<dyn std::error::Error>> {
        let width = encoder.width() as usize;
        let height = encoder.height() as usize;
        let mut rand = rand::thread_rng();

        Ok(Self {
            encoder,
            // TODO: Default??
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
            fps,
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
        let src_fmt = ImageFormat {
            pixel_format: dcv_color_primitives::PixelFormat::Bgra,
            color_space: ColorSpace::Rgb,
            num_planes: 1,
        };
        let dst_fmt = ImageFormat {
            pixel_format: dcv_color_primitives::PixelFormat::I420,
            color_space: ColorSpace::Bt601,
            num_planes: 3,
        };

        let sizes: &mut [usize] = &mut [0usize; 3];
        debug!("frame width {} height {}", buffer.width(), buffer.height());
        get_buffers_size(buffer.width(), buffer.height(), &dst_fmt, None, sizes)?;
        debug!(
            "{:?} and yuv buffer capacity is {}",
            sizes,
            self.yuv_buffer.yuv.capacity()
        );

        let src_sizes: &mut [usize] = &mut [0usize; 1];
        get_buffers_size(buffer.width(), buffer.height(), &src_fmt, None, src_sizes)?;
        debug!(
            "{:?} and src buffer capacity is {}",
            src_sizes,
            buffer.len(),
        );

        let src_strides: &[usize] = &[4 * (buffer.width() as usize)];

        let (mut y_slice, uv_slice) = self.yuv_buffer.yuv.split_at_mut(sizes[0]);
        let (mut u_slice, mut v_slice) = uv_slice.split_at_mut(sizes[1]);

        convert_image(
            buffer.width(),
            buffer.height(),
            &src_fmt,
            Some(src_strides),
            &[&buffer.into_raw()],
            &dst_fmt,
            None,
            &mut [&mut y_slice, &mut u_slice, &mut v_slice],
        )?;
        debug!("convert image sequence is {:.4?}", pre_enc.elapsed());

        let pre_enc = Instant::now();
        let bit_stream = self.encoder.encode_frame(&self.yuv_buffer, timestamp)?;
        debug!("encode_frame elapsed time: {:.4?}", pre_enc.elapsed());
        debug!("h264 bit stream layer count is {}", bit_stream.num_layers());

        debug!("bit stream is {:?}", bit_stream.to_vec());

        let pre_enc = Instant::now();
        // Delete the old packet.
        self.h264_bitstream_writer.get_mut().clear();

        // TODO how to reserve the memory in advance?

        // Put in the bitstream buffer
        let _ = bit_stream.write(&mut self.h264_bitstream_writer);

        debug!(
            "bitstream buffer len after write is {}",
            self.h264_bitstream_writer.get_ref().len(),
        );
        debug!("bitstream buffer write: {:.4?}", pre_enc.elapsed());

        // Extract RTP and put in queue from the bytes
        // The split will be dropped at the end of this function, so when we clear the bitstream writer and write to it later, it will use the whole buffer.
        let pre_enc = Instant::now();
        let unpacketized_payload: Bytes = Bytes::from(self.h264_bitstream_writer.get_mut().split());
        debug!("unpacketized_payload len is {}", unpacketized_payload.len());

        let payloads = self
            .packetizer
            .packetize(&unpacketized_payload, SAMPLE_RATE / self.fps)?;
        debug!("packetization: {:.4?}", pre_enc.elapsed());

        let pre_enc = Instant::now();
        let mut packet_count = 0;
        for payload in payloads {
            // Marshal into RTP.
            debug!("packet payload data: {:?}", &payload.payload.as_ref());
            self.rtp_queue.push_front(payload);
            packet_count += 1;
        }
        debug!("wrote {} RTP packets into queue", packet_count);
        debug!("queuing: {:.4?}", pre_enc.elapsed());

        Ok(())
    }
    // Get the next RTP packet to send over the network
    pub fn pop_rtp(&mut self) -> Option<Packet> {
        self.rtp_queue.pop_back()
    }

    // pub fn encrypt();
    // pub fn error_correct();
}
