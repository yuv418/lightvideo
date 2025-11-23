# Debugging LightVideo

Certain features are provided to help debugging the H.264 stream.

## LVFileDecoder

Edit `src/decoder/video.rs`, namely the `LVDecoder::decode_loop` function. Find the location where the `LVOpenH264Decoder` or `LVVAAPIDecoder` is initialized, and replace it with `LVFileDecoder`.

The `LVFileDecoder`, dumps the raw H264 payload (effectively, what is fed to a "normal" decoder) to a file called `test.h264`. This is _not_ the RTP payload, but what is depacketized and aggregated between RTP partition heads (the `rtp` crate indicates this may be more commonly referred to as the head of a fragmentation unit). 

The `test.h264` file can be used in conjunction with the `ccdec` utility in `cros-codecs` to test whether or not the `cros-codecs` library is unable to decode the bitstream or whether there is an issue in how we invoke the `cros-codecs` library.
