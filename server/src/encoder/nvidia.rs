use std::fs::File;
use std::io::Write;
use std::sync::Arc;

use cudarc::driver::CudaDevice;
use log::{debug, info, trace};
use nvidia_video_codec_sdk::sys::nvEncodeAPI::{
    NV_ENC_BUFFER_FORMAT::*, NV_ENC_H264_PROFILE_BASELINE_GUID, NV_ENC_PIC_FLAGS,
    NV_ENC_PRESET_LOW_LATENCY_HP_GUID,
};
use nvidia_video_codec_sdk::sys::nvEncodeAPI::{
    NV_ENC_CODEC_H264_GUID, NV_ENC_INITIALIZE_PARAMS, NV_ENC_PRESET_P1_GUID, NV_ENC_PRESET_P2_GUID,
};
use nvidia_video_codec_sdk::{
    Bitstream, Buffer, CodecPictureParams, EncodePictureParams, Encoder, Session,
};

use super::LVEncoder;

pub struct LVNvidiaEncoder {
    enc_session: Session,
    // input_buffer: Buffer<'a>,
    // utput_bitstream: Bitstream<'a>,
    width: u32,
    height: u32,
    frame_no: u64,
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

        let mut enc_params = NV_ENC_INITIALIZE_PARAMS::new(NV_ENC_CODEC_H264_GUID, width, height);

        let mut preset_cfg =
            enc.get_preset_config(
                NV_ENC_CODEC_H264_GUID,
                NV_ENC_PRESET_LOW_LATENCY_HP_GUID,
                nvidia_video_codec_sdk::sys::nvEncodeAPI::NV_ENC_TUNING_INFO::NV_ENC_TUNING_INFO_HIGH_QUALITY,
            )?;

        unsafe {
            preset_cfg.presetCfg.profileGUID = NV_ENC_H264_PROFILE_BASELINE_GUID;
            info!(
                "idr period is {}",
                preset_cfg.presetCfg.encodeCodecConfig.h264Config.idrPeriod
            );
            // std::process::exit(0);
            preset_cfg.presetCfg.encodeCodecConfig.h264Config.idrPeriod = 120;
            preset_cfg
                .presetCfg
                .encodeCodecConfig
                .h264Config
                .set_repeatSPSPPS(240);

            preset_cfg.presetCfg.gopLength = 120;

            /*q.profileGUID = NV_ENC_H264_PROFILE_BASELINE_GUID;
            enc_params.encodeCode = q;*/
        }

        // info!("preset cfg is {:?}", preset_cfg.presetCfg.encodeCodecConfig.);

        enc_params.framerate(framerate as u32, 1);
        enc_params.enable_picture_type_decision();
        enc_params.encode_config(&mut preset_cfg.presetCfg);
        //
        let enc_session = enc.start_session(NV_ENC_BUFFER_FORMAT_NV12, enc_params)?;
        info!("NVIDIA encoder has been initialized");

        Ok(Self {
            width,
            height,
            enc_session,
            frame_no: 0,
            // output_bitstream,
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
        buffer: &openh264::formats::YUVBuffer,
        // Milliseconds from start.
        timestamp: u64,
        h264_buffer: &mut bytes::buf::Writer<bytes::BytesMut>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // this doesn't create a memory leak.. right?
        let mut input_buffer = self.enc_session.create_input_buffer()?;
        let mut output_bitstream = self.enc_session.create_output_bitstream()?;

        unsafe {
            let mut i = input_buffer.lock().unwrap();
            i.write(&buffer.yuv);
        }

        debug!("Beginning frame encode");
        debug!("timestamp is {}", timestamp);

        self.enc_session.encode_picture(
            &mut input_buffer,
            &mut output_bitstream,
            if self.frame_no % 240 == 0 {
                debug!("sending spspps");
                ((NV_ENC_PIC_FLAGS::NV_ENC_PIC_FLAG_FORCEIDR as u8)
                    | (NV_ENC_PIC_FLAGS::NV_ENC_PIC_FLAG_OUTPUT_SPSPPS as u8)
                    | (NV_ENC_PIC_FLAGS::NV_ENC_PIC_FLAG_FORCEINTRA as u8))
                    .into()
            } else {
                0
            },
            EncodePictureParams {
                input_timestamp: timestamp,
                ..Default::default()
            },
        )?;

        debug!("Finished frame encode");

        let bs_lock = output_bitstream.lock().unwrap();
        let h264_data = bs_lock.data();

        h264_buffer.write(&h264_data);

        dbg!(bs_lock.duration());
        dbg!(bs_lock.frame_index());
        dbg!(bs_lock.picture_type());
        dbg!(bs_lock.timestamp());

        trace!("h264_buffer is {:?}", h264_buffer.get_ref().len());
        self.frame_no += 1;

        Ok(())
    }
}
