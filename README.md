# LightVideo

Started as a game streaming/remote desktop system, and works as such. It is also, to some degree, a "testbench" to experiment with techniques for better low-latency video streaming with minimal packet loss.

## Approach

LightVideo uses a couple of techniques to achieve low-latency streaming:

1. Forward error correction (using Reed-Solomon erasure codes). This makes packet loss less likely to crash the video decoder.
2. Variable bitrate streaming, dealing with network condition changes. Currently uses packet loss as the congestion signal.

There is also a focus on minimizing heap allocations and copies for this goal. 

## Usage

LightVideo requires manual set up. Because of firewall issues in my configuration, I made it so the feedback TCP server is the client. This is for testing purposes.
The client must be started first.

On the client (in the `client/` directory):
```
cargo run --release 0.0.0.0:17728
```

In terms of server ports, streaming is at port `17728`, input is at port `17729`, and feedback is at port `17730`. The latter two port numbers are always an offset of the streaming port number.

On the server, if you want software encoding.
```
cargo run --release -- server 0.0.0.0:17789 CLIENT_IP_ADDR:17728
```


If you want hardware encoding (NVENC is currently the only one supported):

```
CUDA_PATH=/usr/local/cuda-<VER>/targets/x86_64-linux/include/ cargo run --release --features nvidia-hwenc -- server 0.0.0.0:17789 CLIENT_IP_ADDR:17728
```

The `0.0.0.0:17789` parameter is a vestige from before certain firewall rules happened and I had to move things so the server binds to the client address. I believe it is not used. All the ports/binding locations are offsets of `CLIENT_IP_ADDR:17728`.

NICE DCV's pixel format conversion library runs much faster if you compile in release mode (from what I remember), so it is useful to do that. At some point, we  might try to use STUN/TURN servers to make this more ergonomic and accessible.

## Organization

**Statistics (in `statistics/src/collector.rs`):** This is a low-overhead measurement system that aggregates together metrics from either the client or server and logs them to a file at the end. Metrics are collected with the API in `collector.rs`. There are also some Python scripts here to help visualise the output of the measurement system. The measurement system can either count an "aggregate" (eg. total packets lost over time), a "time" series (eg. a list of how long each video decode took), or "x-y data" (where you can specify an (x,y) value of floats to log).

**Client:** This receives video stream data and sends feedback/input. It uses `wgpu` to copy the decoded video to the GPU for display (this could probably be made faster).

- The `ui` module contains the GPU/UI code.
- The `decoder` module contains code for sending feedback/input. 
    - The file `video.rs` receives data in a loop from `network.rs`. It recovers any lost data using erasure codes. Then, it depacketizes and at the start of a new fragmentation unit sends the H264 data to the decoder. The decoder sets the new frame in a double buffer (connected to GPU code) and swaps. It is also responsible for updating feedback (lost packets, out of order blocks, packets recovered, etc.) that is used for adaptive bitrate streaming.
    - The `video_decoder` directory contains genericized code that works for multiple video decoder backends, and implementations for those backends.
    - The file `input.rs` sends input over the network, receiving this from the `ui` module.
    - The file `feedback.rs` sends feedback over the network. This feedback is data that is shared between `video.rs` and `feedback.rs` with a mutex.


**Server:** Runs on the system that wants to share its screen. It has components to:

- Capture the screen (in `server/src/capture/`). Only X11-based capture is supported right now, but it should be straightforward to add support for other things (eg. [host-based NVIDIA vGPU capture from the old rendition of this project](https://github.com/yuv418/echodawn/tree/master/EDSS/CAL/vgpu))
- Encoder (in `server/src/encoder/`). This has the code that implements a generic interface for video encode. Each implementation must do pixel format conversions themselves before outputting. See the Support section for some details. The encoder interface trait is in `mod.rs`.
- Input (in `server/src/encoder/`). This takes in an input event that the client sends, and simulates it. Currently works only for X11, but there is an interface to make this functional on other platforms in `mod.rs`.
- Packager (in `server/src/encoder/`): This takes an RTP packet, encapsulates/transforms extra data, and sends it over the wire. Currently encapsulation adds a header to indicate whether the current packet is RTP or error correcting information. The packager also adds/produces recovery packets. 
- Server (in `server/src/encoder/`): Contains the main loops that do socket communications. There are **three** servers: feedback, streaming, and input. The streaming server runs over UDP. The feedback server is over TCP, and the input server is over UDP. Note that the feedback server also sends ACK packets for packets sent from the server. 

**Net:** Contains common structures for all network communication (feedback, streaming, input). Note `net/src/packet.rs` contains the error correction ratio. 

The Rust bindings for NVENC are forked to use reference counting. Otherwise, there is performance (confirmed thanks to `perf`) overhead from having to reinitialize the encoder repeatedly due to Rust ownership issues. 


## Metrics

The following are some metrics tracked by the client/server using the statistics module, and descriptions of those metrics. Some of the metrics are redundant or just for testing things out.
For more info on the metric types and such, look at the Organization section.

### Client

- `client_packets_out_of_order`: aggregate metric for packets that come in out of order. Might not be logged correctly. 
- `client_decode_packet`: The time (time series) it takes to run the `depacketize_decode` function, which depacketizes H264 data from RTP and (if at the head of a new fragmentation unit in RTP) decodes H264. 
- `client_failed_decode_packets`: Only used by OpenH264 decoder currently, and aggregates the total number of H264 NAL units that caused an error when sent to OpenH264. 
- `client_backend_decode_time`: the time (time series) it takes to decode H264 NALUs (calling the video decoder's decode function with a NAL unit), as opposed to `client_decode_packet` which accounts for RTP depacketization time as well.

### Server

- `server_packet_sending`: time series that records the time it takes to send all the RTP packets for one NAL unit over UDP.
- `server_bitrate_queue_occupancy`: Was used for a congestion signal experiment. XY data, X = bitrate, Y = average output queue occupancy (based on TIOCOUTQ)
- `server_bitrate_oo_blocks`: XY, X = current bitrate, Y = number of out of order blocks (this comes from the client)
- `server_rtt_time`: "time" series. RTT calculated by looking at ack packets (acks are a type of feedback packets; see Organization)
- `server_rtt_bitrate`: XY data, X = bitrate, Y = rtt
- `server_bitrate_ecc_decoder_failures`: XY data, X = bitrate, Y = # of ecc decoder failures (comes from feedback)
- `server_packetization`: The time it takes to convert a captured frame to the right pixel format, send to encoder, and packetize to RTP
- `server_queuing`: Time it takes to push the packetized RTP into a queue of RTPs
- `server_allocate_frames`: only used by NVENC to see how long it takes to allocate the input framebuffer and output bitstream bufffer. The point of this was to understand how the overhead of re-allocating this during every encode (caused by ownership issues in the NVENC Rust wrapper that were fixed). "Time" series.
- `server_encode_frame`: the time ("time" series) it takes to encode one frame in the encoder. 
- `server_bitstream_buffer_write`: how long does it take to write the encoded video data to a bitstream buffer? "Time" series.

## Demo

https://github.com/user-attachments/assets/4a066168-e143-4d38-9a0b-957c26ed8023


For context: with current experimental parameters, the decoder takes a while to get to a high bitrate (and might not if the network conditions are bad). Yes, there is a small mouse offset bug. This is running on NVENC encoder and a OpenH264 decoder.

We use 60FPS video game gameplay on YouTube, since there is a lot of motion in the video. 

## Support

Currently LightVideo supports the following:

- Encoders: **openh264 (SW)**, **NVENC (HW)**
- Decoders: **openh264 (SW)**, **VA-API (HW)**

The VA-API decoder works but needs some tuning. 

## Future Work

- Congestion signals. Packet loss is maybe not the best congestion signal since we want to prevent issues before a huge burst of packets takes down the encoder. I remmeber reading a paper saying that using jitter as a congestion signal could be interesting. I was also suggested to observe queuing occupancy to do this, but this is hard to measure.
- Different FEC schemes, specifically convolutional codes (eg. [CauchyCaterpillar](https://github.com/catid/CauchyCaterpillar)). Block codes, from my experience, require reorder buffers and extra copying, it would be preferrable to avoid these things in the sake of lowering latency.
- Aforementioned STUN/TURN servers.
