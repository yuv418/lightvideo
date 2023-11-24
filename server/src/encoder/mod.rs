use std::os::raw::c_int;

use openh264::{
    encoder::{EncodedBitStream, Encoder},
    formats::YUVSource,
    Error as OpenH264Error, Timestamp,
};

use openh264_sys2::{SEncParamExt, LOW_COMPLEXITY, RC_BITRATE_MODE};

pub struct LVEncoder {
    encoder: Encoder,
    width: u32,
    height: u32,
}

// The primary purpose of this is to tune the Encoder parameters in one place.
impl LVEncoder {
    pub fn new(
        width: u32,
        height: u32,
        bitrate: u32,
        framerate: f32,
    ) -> Result<Self, OpenH264Error> {
        let mut params = SEncParamExt::default();

        params.iPicWidth = width as c_int;
        params.iPicHeight = height as c_int;
        params.iRCMode = RC_BITRATE_MODE;
        params.iComplexityMode = LOW_COMPLEXITY;
        params.bEnableFrameSkip = false;
        params.iTargetBitrate = bitrate as c_int;
        params.bEnableDenoise = false;
        params.fMaxFrameRate = framerate;
        params.bEnableAdaptiveQuant = false;
        params.bEnableAdaptiveQuant = false;
        params.iMultipleThreadIdc = 8;
        params.iEntropyCodingModeFlag = 0;
        // GOP Size
        params.uiIntraPeriod = 120;
        // Quantization parameters?
        params.iMinQp = 21;
        params.iMinQp = 35;

        let _true_val = true;
        let _false_val = false;

        unsafe {
            let mut encoder = Encoder::with_raw_config(params)?;
            let _raw_api = encoder.raw_api();
            // raw_api.set_option(ENCODER_OPTION_TRACE_LEVEL, addr_of_mut!(false_val).cast());
            // raw_api.set_option(ENCODER_OPTION_DATAFORMAT, addr_of_mut!(true_val).cast());
            Ok(LVEncoder {
                encoder,
                width,
                height,
            })
        }
    }
    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    pub fn encode_frame<T: YUVSource>(
        &mut self,
        buffer: &T,
        // Milliseconds from start.
        timestamp: u64,
    ) -> Result<EncodedBitStream, OpenH264Error> {
        self.encoder
            .encode_at(buffer, Timestamp::from_millis(timestamp))
    }
}
