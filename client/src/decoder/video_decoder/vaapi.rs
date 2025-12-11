use std::{rc::Rc, sync::Arc};

use anyhow::anyhow;
use cros_codecs::{
    backend::vaapi::decoder::VaapiBackend,
    decoder::{
        DecodedHandle, DecoderEvent,
        stateless::{DecodeError, PoolLayer, StatelessDecoder, StatelessVideoDecoder, h264::H264},
    },
    libva::{Display, Image},
};
use dcv_color_primitives::ImageFormat;
use log::{error, info, trace, warn};
use nix::libc::stack_t;
use openh264::decoder::DecodedYUV;

use crate::double_buffer::DoubleBuffer;

use super::{LVVideoDecoder, imgfmt_converter::ImageFormatConverter};

pub struct LVVAAPIDecoder {
    width: u32,
    height: u32,
    decoder: StatelessDecoder<H264, VaapiBackend<()>>,
    double_buffer: Arc<DoubleBuffer>,
    src_format: ImageFormat,
    dst_format: ImageFormat,
    yuv_buffer: Vec<u8>,
    imgfmt_converter: Option<ImageFormatConverter>,
    has_setup_buffers: bool,
    frame_num: u64,
}

impl LVVideoDecoder for LVVAAPIDecoder {
    fn new(
        src_format: ImageFormat,
        dst_format: ImageFormat,
        double_buffer: Arc<DoubleBuffer>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        match Display::open() {
            Some(disp) => {
                let decoder: StatelessDecoder<H264, VaapiBackend<()>> =
                    StatelessDecoder::<H264, VaapiBackend<()>>::new_vaapi(
                        disp,
                        cros_codecs::BlockingMode::Blocking,
                    )?;
                trace!("vaapi disp and decoder created");

                Ok(Self {
                    width: 0,
                    height: 0,
                    frame_num: 0,
                    decoder,
                    double_buffer,
                    imgfmt_converter: None,
                    yuv_buffer: vec![],
                    src_format,
                    has_setup_buffers: false,
                    dst_format,
                })
            }
            None => Err(anyhow!("failed to open VA-API display").into()),
        }
    }

    fn decode(&mut self, timestamp: u64, packet: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        // drain event q

        trace!("timestamp {timestamp}");
        let mut ret_val = Ok(());
        let mut packet_window = packet;

        let mut process_events = |decoder: &mut StatelessDecoder<H264, VaapiBackend<()>> | -> Result<(), Box<dyn std::error::Error>> {
            trace!("in check events...");

            let mut events = 0;
            let mut last_frame = None;

            while let Some(event) = decoder.next_event() {
                // For performance reasons, if VA-API is going to give us multiple frames,
                // then we should choose the last one to display, to avoid extra pixel format
                // conversions
                events += 1;

                match event {
                    DecoderEvent::FrameReady(frame) => {
                        trace!("a frame is ready!");

                        last_frame = Some(frame.clone());
                    }
                    // TODO no cloning
                    DecoderEvent::FormatChanged(mut format_setter) => {
                        // WARN: this should run again if the format has actually changed
                        // (eg. the stream was not corrupted)
                        if !self.has_setup_buffers {
                            info!("setting format to {:?}", format_setter.stream_info().format);
                            format_setter.try_format(format_setter.stream_info().format)?;
                            let min_num_frames = {
                                let sinfo = format_setter.stream_info();
                                self.width = sinfo.display_resolution.width;
                                self.height = sinfo.display_resolution.height;
                                self.double_buffer.initialize(
                                    (4 * self.width * self.height) as usize,
                                    self.width as usize,
                                    self.height as usize,
                                );
                                sinfo.min_num_frames
                            };

                            let pools = format_setter.frame_pool(PoolLayer::All);
                            let nb_pools = pools.len();

                            info!("there are {nb_pools} pools");

                            for pool in pools {
                                let pool_num_frames = pool.num_managed_frames();
                                info!(
                                    "there are {pool_num_frames} frames min frame {min_num_frames}"
                                );
                                if pool_num_frames < (min_num_frames / nb_pools) {
                                    info!(
                                        "adding {} frames to pool",
                                        min_num_frames - pool_num_frames
                                    );
                                    pool.add_frames(vec![
                                                ();
                                                (min_num_frames - pool_num_frames)
                                            ])?;
                                }
                            }

                            // Initialize stuff that converts from I420 to RGBA or whatever the
                            // target format is

                            self.imgfmt_converter = Some(ImageFormatConverter::new(
                                ImageFormat { ..self.src_format },
                                ImageFormat { ..self.dst_format },
                                self.width,
                                self.height,
                            ))
                        }
                    }
                }
            }

            // In the case of a frame, process it now.
            if let Some(frame) = last_frame {
                trace!("swapping last frame");
                {
                    let mut db_frame = self.double_buffer.back().unwrap();

                    frame.sync()?;

                    let dyn_pic = frame.dyn_picture();
                    let mut mappable_handle = dyn_pic.dyn_mappable_handle()?;

                    if self.yuv_buffer.len() != mappable_handle.image_size() {
                        self.yuv_buffer = vec![0; mappable_handle.image_size()];
                    }
                    mappable_handle.read(&mut self.yuv_buffer)?;

                    // We need to convert from I420 to RGBA or whatever.
                    // perform some slicing

                    let y_end = (self.height * self.width) as usize;
                    let y = &self.yuv_buffer[..y_end];
                    // dangerous variable name
                    let u_size = (self.height / 2 * self.width / 2) as usize;
                    let u = &self.yuv_buffer[y_end..y_end + u_size];
                    let v = &self.yuv_buffer[y_end + u_size..];

                    self.imgfmt_converter.as_mut().unwrap().convert(
                        y,
                        u,
                        v,
                        &mut db_frame.as_mut().unwrap().buffer,
                    )?;
                }
                self.double_buffer.swap();
            }
            trace!("processed {} events", events);

            Ok(())
        };

        while packet_window.len() > 0 {
            trace!("packet window length is {}", packet_window.len());
            loop {
                let mut must_retry_decode = false;
                let decode_result = self.decoder.decode(timestamp, packet_window);

                ret_val = match decode_result {
                    Ok(amt_decoded) => {
                        packet_window = &packet_window[amt_decoded..];

                        if let Some(stream_info) = self.decoder.stream_info() {
                            trace!(
                                "stream info is {:?} and amount decoded is {}",
                                stream_info.format, amt_decoded
                            );

                            process_events(&mut self.decoder)?;

                            Ok(())
                        } else {
                            warn!("could not get stream info, was none");
                            Ok(())
                        }
                    }
                    // delete this, makes zero sense
                    Err(DecodeError::CheckEvents) => {
                        trace!("decoder told us to check events");
                        must_retry_decode = true;

                        process_events(&mut self.decoder)
                    }
                    Err(DecodeError::NotEnoughOutputBuffers(_)) => {
                        trace!(
                            "not enough buffers, means frame is ready probably, processing events..."
                        );
                        process_events(&mut self.decoder)
                    }
                    Err(DecodeError::DecoderError(x)) => {
                        error!("failed to decode packet with error {:#?}", x);
                        // self.decoder.flush()?;
                        Ok(())
                    }
                    Err(e) => {
                        panic!("couldn't decode for reason {e} {e:?}");
                        Err(Box::new(e))
                    }
                };

                trace!("must retry decode {must_retry_decode}");
                if !must_retry_decode {
                    break;
                } else {
                    trace!("retrying decode because there were events")
                }
            }
        }

        Ok(ret_val?)
    }
}
