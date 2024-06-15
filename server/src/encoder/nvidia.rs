use std::fs::File;
use std::io::Write;
use std::sync::Arc;
use std::time::Instant;

use cudarc::driver::CudaDevice;
use dcv_color_primitives::{convert_image, get_buffers_size, ImageFormat};
use image::{ImageBuffer, Rgb};
use log::{debug, error, info, trace};
use nvidia_video_codec_sdk::sys::nvEncodeAPI::{
    NV_ENC_BUFFER_FORMAT::*, NV_ENC_H264_PROFILE_BASELINE_GUID, NV_ENC_PIC_FLAGS,
    NV_ENC_PRESET_LOW_LATENCY_HP_GUID,
};
use nvidia_video_codec_sdk::sys::nvEncodeAPI::{
    NV_ENC_CODEC_H264_GUID, NV_ENC_INITIALIZE_PARAMS, NV_ENC_PRESET_P1_GUID, NV_ENC_PRESET_P2_GUID,
    NV_ENC_RECONFIGURE_PARAMS_VER, _NV_ENC_PARAMS_RC_MODE::NV_ENC_PARAMS_RC_CBR,
    _NV_ENC_RECONFIGURE_PARAMS,
};
use nvidia_video_codec_sdk::{
    Bitstream, Buffer, CodecPictureParams, EncodeError, EncodePictureParams, Encoder, ErrorKind,
    Session,
};
use openh264::formats::YUVBuffer;
use statistics::collector::LVStatisticsCollector;
use statistics::statistics::{LVDataPoint, LVDataType};

use super::LVEncoder;

pub struct LVNvidiaEncoder {
    enc_session: Session,
    input_buffer: Buffer,
    output_bitstream: Bitstream,
    width: u32,
    height: u32,
    frame_no: u64,

    // parameters
    enc_params: NV_ENC_INITIALIZE_PARAMS,

    // image conversion stuff
    src_fmt: ImageFormat,
    dst_fmt: ImageFormat,
    src_strides: [usize; 1],
    out_sizes: [usize; 3],
}

impl LVEncoder for LVNvidiaEncoder {
    fn new(
        width: u32,
        height: u32,
        bitrate: u32,
        framerate: f32,
    ) -> Result<Self, Box<dyn std::error::Error>>
    where
        Self: Sized,
    {
        let dev = CudaDevice::new(0)?;
        let enc = Encoder::initialize_with_cuda(dev)?;

        LVStatisticsCollector::register_data("server_allocate_frames", LVDataType::TimeSeries);
        LVStatisticsCollector::register_data("server_encode_frame", LVDataType::TimeSeries);
        LVStatisticsCollector::register_data(
            "server_bitstream_buffer_write",
            LVDataType::TimeSeries,
        );

        let mut enc_params = NV_ENC_INITIALIZE_PARAMS::new(NV_ENC_CODEC_H264_GUID, width, height);

        let mut preset_cfg =
            enc.get_preset_config(
                NV_ENC_CODEC_H264_GUID,
                NV_ENC_PRESET_LOW_LATENCY_HP_GUID,
                nvidia_video_codec_sdk::sys::nvEncodeAPI::NV_ENC_TUNING_INFO::NV_ENC_TUNING_INFO_ULTRA_LOW_LATENCY,
            )?;

        let src_fmt = ImageFormat {
            pixel_format: dcv_color_primitives::PixelFormat::Bgra,
            color_space: dcv_color_primitives::ColorSpace::Rgb,
            num_planes: 1,
        };
        let dst_fmt = ImageFormat {
            pixel_format: dcv_color_primitives::PixelFormat::Nv12,
            color_space: dcv_color_primitives::ColorSpace::Bt601,
            num_planes: 1,
        };

        let mut out_sizes = [0usize; 3];

        get_buffers_size(width, height, &dst_fmt, None, &mut out_sizes)?;

        let src_strides = [4 * (width as usize)];

        unsafe {
            preset_cfg.presetCfg.profileGUID = NV_ENC_H264_PROFILE_BASELINE_GUID;
            info!(
                "idr period is {}",
                preset_cfg.presetCfg.encodeCodecConfig.h264Config.idrPeriod
            );
            preset_cfg
                .presetCfg
                .encodeCodecConfig
                .h264Config
                .maxNumRefFrames = 1;
            preset_cfg.presetCfg.encodeCodecConfig.h264Config.sliceMode = 0;
            preset_cfg.presetCfg.rcParams.rateControlMode = NV_ENC_PARAMS_RC_CBR;
            preset_cfg.presetCfg.rcParams.averageBitRate = bitrate;
            preset_cfg
                .presetCfg
                .encodeCodecConfig
                .h264Config
                .sliceModeData = 0;
            preset_cfg.presetCfg.encodeCodecConfig.h264Config.idrPeriod = 300;
            preset_cfg.presetCfg.gopLength = 300;

            preset_cfg
                .presetCfg
                .encodeCodecConfig
                .h264Config
                .set_repeatSPSPPS(1);
            preset_cfg
                .presetCfg
                .encodeCodecConfig
                .h264Config
                .set_enableIntraRefresh(1);
            preset_cfg
                .presetCfg
                .encodeCodecConfig
                .h264Config
                .intraRefreshPeriod = 300;
            preset_cfg
                .presetCfg
                .encodeCodecConfig
                .h264Config
                .intraRefreshCnt = 30;

            // Setting frameInter   valP messes with things, namely it makes the encoder never output P frames, or anythign past
            // the first SPS/PPS
            // preset_cfg.presetCfg.frameIntervalP = 300;

            /*q.profileGUID = NV_ENC_H264_PROFILE_BASELINE_GUID;
            enc_params.encodeCode = q;*/
        }

        // info!("preset cfg is {:?}", preset_cfg.presetCfg.encodeCodecConfig.);

        enc_params.framerate(framerate as u32, 1);
        enc_params.enable_picture_type_decision();
        enc_params.encode_config(&mut preset_cfg.presetCfg);

        //
        let enc_session = enc.start_session(NV_ENC_BUFFER_FORMAT_NV12, &mut enc_params)?;
        info!("NVIDIA encoder has been initialized");

        let pre_enc = Instant::now();
        let mut input_buffer = enc_session.create_input_buffer()?;
        let mut output_bitstream = enc_session.create_output_bitstream()?;
        LVStatisticsCollector::update_data(
            "server_allocate_frames",
            LVDataPoint::TimeElapsed(pre_enc.elapsed()),
        );

        Ok(Self {
            width,
            height,
            enc_session,
            enc_params,
            frame_no: 0,
            src_fmt,
            dst_fmt,
            src_strides,
            out_sizes,
            input_buffer,
            output_bitstream,
        })
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
        h264_buffer: &mut bytes::buf::Writer<bytes::BytesMut>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // this doesn't create a memory leak.. right?

        unsafe {
            let mut i = self.input_buffer.lock().unwrap();
            i.write(&buffer.yuv);
        }

        debug!("Beginning frame encode");
        debug!("timestamp is {}", timestamp);

        let pre_enc = Instant::now();

        match self.enc_session.encode_picture(
            &mut self.input_buffer,
            &mut self.output_bitstream,
            if self.frame_no % 120 == 0 {
                0
                /*debug!("sending spspps");
                ((NV_ENC_PIC_FLAGS::NV_ENC_PIC_FLAG_FORCEIDR as u8)
                    | (NV_ENC_PIC_FLAGS::NV_ENC_PIC_FLAG_OUTPUT_SPSPPS as u8)
                    | (NV_ENC_PIC_FLAGS::NV_ENC_PIC_FLAG_FORCEINTRA as u8))
                    .into()*/
            } else {
                0
            },
            EncodePictureParams {
                input_timestamp: timestamp,
                ..Default::default()
            },
        ) {
            Err(e) if e.kind() == ErrorKind::NeedMoreInput => Ok(()),
            Ok(()) => {
                LVStatisticsCollector::update_data(
                    "server_encode_frame",
                    LVDataPoint::TimeElapsed(pre_enc.elapsed()),
                );
                debug!("Finished frame encode");

                let bs_lock = self.output_bitstream.lock().unwrap();
                let h264_data = bs_lock.data();

                let pre_enc = Instant::now();

                h264_buffer.write(&h264_data);

                LVStatisticsCollector::update_data(
                    "server_bitstream_buffer_write",
                    LVDataPoint::TimeElapsed(pre_enc.elapsed()),
                );
                debug!("bs_lock.duration() = {:?}", bs_lock.duration());
                debug!("bs_lock.frame_index() = {:?}", bs_lock.frame_index());
                debug!("bs_lock.picture_type() = {:?}", bs_lock.picture_type());
                debug!("bs_lock.timestamp() = {:?}", bs_lock.timestamp());

                trace!("h264_buffer is {:?}", h264_buffer.get_ref().len());
                self.frame_no += 1;

                Ok(())
            }
            _ => Ok(()),
        }
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
    fn bitrate(&self) -> u32 {
        unsafe { *self.enc_params.encodeConfig }
            .rcParams
            .averageBitRate
    }
    fn set_bitrate(&mut self, new_bitrate: u32) -> Result<(), Box<dyn std::error::Error>> {
        unsafe {
            (*self.enc_params.encodeConfig).rcParams.averageBitRate = new_bitrate;
        }

        debug!("x {:?}", unsafe { *self.enc_params.encodeConfig }.rcParams);

        let mut reconfigure_params = _NV_ENC_RECONFIGURE_PARAMS {
            version: NV_ENC_RECONFIGURE_PARAMS_VER,
            reInitEncodeParams: self.enc_params,
            ..Default::default()
        };

        reconfigure_params.set_resetEncoder(1);
        reconfigure_params.set_forceIDR(1);

        match self
            .enc_session
            .get_encoder()
            .reconfigure_encoder(reconfigure_params)
        {
            Ok(k) => debug!("finished reconfiguring encoder!"),
            Err(e) => error!("failed to set bitrate {:?}", e),
        }

        Ok(())
    }
}
