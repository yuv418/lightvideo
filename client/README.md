# client

notes:

## ffplay decode


filename: `test_new.sdp`

``` sdp
c=IN IP4 IN_IP
m=video SECOND_PORT_PARAM_TO_SERVER RTP/AVP 96
a=rtpmap:96 H264/90000
```

Command:

`ffplay -fflags nobuffer -protocol_whitelist file,udp,rtp  test_new.sdp`

used to check if decoder or server is causing bugs.
