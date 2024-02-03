use std::{io::ErrorKind, os::raw::c_int, time::Instant};

use bytes::{buf::Writer, BytesMut};
use dcv_color_primitives::{convert_image, get_buffers_size, ImageFormat};
use image::{ImageBuffer, Rgb};
use log::debug;
use openh264::{
    encoder::{EncodedBitStream, Encoder},
    formats::{YUVBuffer, YUVSource},
    Error as OpenH264Error, Timestamp,
};

use openh264_sys2::{SEncParamExt, LOW_COMPLEXITY, RC_BITRATE_MODE};
use statistics::{
    collector::LVStatisticsCollector,
    statistics::{LVDataPoint, LVDataType},
};

use super::LVEncoder;

pub struct LVOpenH264Encoder {
    encoder: Encoder,
    width: u32,
    height: u32,

    // Image conversion stuff
    src_fmt: ImageFormat,
    dst_fmt: ImageFormat,
    src_strides: [usize; 1],
    out_sizes: [usize; 3],
}

// The primary purpose of this is to tune the Encoder parameters in one place.
impl LVEncoder for LVOpenH264Encoder {
    fn new(
        width: u32,
        height: u32,
        bitrate: u32,
        framerate: f32,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let mut params = SEncParamExt::default();

        params.iPicWidth = width as c_int;
        params.iPicHeight = height as c_int;
        // params.iRCMode = RC_BITRATE_MODE;
        params.iComplexityMode = LOW_COMPLEXITY;
        params.bEnableFrameSkip = false;
        params.iTargetBitrate = bitrate as c_int;
        params.bEnableDenoise = true;
        params.fMaxFrameRate = framerate;
        params.bEnableAdaptiveQuant = false;
        params.iMultipleThreadIdc = 8;
        params.iEntropyCodingModeFlag = 0;
        // GOP Size
        params.uiIntraPeriod = 120;

        // Quantization parameters?
        params.iMinQp = 21;
        params.iMinQp = 35;

        let src_fmt = ImageFormat {
            pixel_format: dcv_color_primitives::PixelFormat::Bgra,
            color_space: dcv_color_primitives::ColorSpace::Rgb,
            num_planes: 1,
        };
        let dst_fmt = ImageFormat {
            pixel_format: dcv_color_primitives::PixelFormat::I420,
            color_space: dcv_color_primitives::ColorSpace::Bt601,
            num_planes: 3,
        };

        let mut out_sizes = [0usize; 3];

        get_buffers_size(width, height, &dst_fmt, None, &mut out_sizes)?;

        let src_strides = [4 * (width as usize)];

        LVStatisticsCollector::register_data("server_encode_frame", LVDataType::TimeSeries);
        LVStatisticsCollector::register_data(
            "server_bitstream_buffer_write",
            LVDataType::TimeSeries,
        );

        unsafe {
            let mut encoder = Encoder::with_raw_config(params)?;
            let _raw_api = encoder.raw_api();
            // raw_api.set_option(ENCODER_OPTION_TRACE_LEVEL, addr_of_mut!(false_val).cast());
            // raw_api.set_option(ENCODER_OPTION_DATAFORMAT, addr_of_mut!(true_val).cast());
            Ok(Self {
                encoder,
                width,
                height,

                dst_fmt,
                src_fmt,
                out_sizes,
                src_strides,
            })
        }
    }
    fn width(&self) -> u32 {
        self.width
    }
    fn height(&self) -> u32 {
        self.height
    }
    fn encode_frame(
        &mut self,
        buffer: &YUVBuffer,
        // Milliseconds from start.
        timestamp: u64,
        h264_buffer: &mut Writer<BytesMut>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let pre_enc = Instant::now();
        let data = self
            .encoder
            .encode_at(buffer, Timestamp::from_millis(timestamp))
            .map_err(|e| Box::new(std::io::Error::new(ErrorKind::InvalidData, e.to_string())))?;

        LVStatisticsCollector::update_data(
            "server_encode_frame",
            LVDataPoint::TimeElapsed(pre_enc.elapsed()),
        );

        debug!("h264 bit stream layer count is {}", data.num_layers());
        debug!("bit stream is {:?}", data.to_vec());

        let pre_enc = Instant::now();
        data.write(h264_buffer);

        LVStatisticsCollector::update_data(
            "server_bitstream_buffer_write",
            LVDataPoint::TimeElapsed(pre_enc.elapsed()),
        );
        Ok(())
    }

    fn convert_frame(
        &mut self,
        input_buffer: ImageBuffer<Rgb<u8>, Vec<u8>>,
        output_buffer: &mut YUVBuffer,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (mut y_slice, uv_slice) = output_buffer.yuv.split_at_mut(self.out_sizes[0]);
        let (mut u_slice, mut v_slice) = uv_slice.split_at_mut(self.out_sizes[1]);

        convert_image(
            input_buffer.width(),
            input_buffer.height(),
            &self.src_fmt,
            Some(&self.src_strides),
            &[&input_buffer.into_raw()],
            &self.dst_fmt,
            None,
            &mut [&mut y_slice, &mut u_slice, &mut v_slice],
        )?;

        Ok(())
    }
}
