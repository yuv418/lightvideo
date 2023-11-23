use std::{collections::VecDeque, time::Instant};

use bytes::{buf::Writer, BufMut, Bytes, BytesMut};
use dcv_color_primitives::{convert_image, get_buffers_size, ColorSpace, ImageFormat};
use image::{ImageBuffer, Rgb};
use log::debug;
use openh264::formats::{YUVBuffer, YUVSource};
use rtp::{codecs::h264::H264Payloader, packetizer::Payloader};

use crate::encoder::LVEncoder;

const MTU_SIZE: usize = 1200;

// TODO update the error handling

// Encode -> RTP Encapsulation -> Encrypt (skip for now) -> Error Correct (skip for now)
pub struct LVPackager {
    encoder: LVEncoder,
    h264_bitstream_writer: Writer<BytesMut>,
    yuv_buffer: YUVBuffer,

    rtp_packet_builder: H264Payloader,

    // TODO: Can we minimize the number of heap allocations with this?
    rtp_queue: VecDeque<Bytes>,
}

//
impl LVPackager {
    pub fn new(encoder: LVEncoder) -> Result<Self, Box<dyn std::error::Error>> {
        let width = encoder.width() as usize;
        let height = encoder.height() as usize;
        Ok(Self {
            encoder,
            // TODO: Default??
            h264_bitstream_writer: BytesMut::new().writer(),
            rtp_packet_builder: H264Payloader::default(),
            rtp_queue: VecDeque::new(),
            yuv_buffer: YUVBuffer::new(width, height),
        })
    }

    // Encode frame and add to RTP queue
    pub fn process_frame(
        &mut self,
        buffer: ImageBuffer<Rgb<u8>, &[u8]>,
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
            .rtp_packet_builder
            .payload(MTU_SIZE, &unpacketized_payload)?;
        debug!("packetization: {:.4?}", pre_enc.elapsed());

        let pre_enc = Instant::now();
        let mut packet_count = 0;
        for payload in payloads {
            self.rtp_queue.push_front(payload);
            packet_count += 1;
        }
        debug!("wrote {} RTP packets into queue", packet_count);
        debug!("queuing: {:.4?}", pre_enc.elapsed());

        Ok(())
    }
    // Get the next RTP packet to send over the network
    pub fn pop_rtp(&mut self) -> Option<Bytes> {
        self.rtp_queue.pop_front()
    }

    // pub fn encrypt();
    // pub fn error_correct();
}
