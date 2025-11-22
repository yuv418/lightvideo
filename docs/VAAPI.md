# VA-API

VAAPI (decode for now) support is provided by `cros-codecs`. To debug it, we can use the `LVFileDecoder`, which dumps the depacketized H264 payload (effectively, what is fed to a "normal" decoder) to a file called `test.h264`. This file can be used in conjunction with the `ccdec` utility in `cros-codecs` to test whether or not the `cros-codecs` library is unable to decode the bitstream or whether there is an issue in how we invoke the `cros-codecs` library.

The following script uses `ccdec` to output a sequence of decoded NV12 frames from the original `test.h264` bitstream:

```bash
#!/bin/bash

# Test decode an h264 dump from remote desktop software
RUST_LOG=trace cargo run --example ccdec -- --input-format h264 --output-format nv12 <PATH TO H264 DUMP (test.h264)> --output test/test_frame --multiple-output-files
ffmpeg -f rawvideo -pix_fmt nv12 -s 1920x1080 -i test/test_frame_0 output.png
gwenview output.png
```

The `trace` logging is useful to understand what `cros-codecs` is doing under the hood.
