use bytes::{BufMut, BytesMut};
use flume::{Receiver, Sender, TryRecvError};
use image::{ImageBuffer, Rgb};
use libc::TIOCOUTQ;
use log::{debug, error, info, trace, warn};
use nix::ioctl_read_bad;
use screenshots::Screen;
use statistics::{
    collector::LVStatisticsCollector,
    statistics::{LVDataPoint, LVDataType},
};
use std::{
    net::UdpSocket,
    os::fd::{AsRawFd, RawFd},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
use webrtc_util::{Marshal, MarshalSize};

use crate::{
    capture::{linux::LVLinuxCapturer, LVCapturer},
    encoder,
    packager::LVPackager,
};

ioctl_read_bad!(tiocoutq, TIOCOUTQ, u32);

pub struct LVStreamingServer {
    bind_addr: String,
    target_addr: String,
    fps: u32,
    screen_no: usize,
    width: u32,
    height: u32,
    quit_rx: Receiver<bool>,
    old_bitrate: u32,
    bitrate_mtx: Arc<Mutex<u32>>,
    udp_fd: Option<RawFd>,

    // queue-occupancy/bitrate tradeoff
    total_queue_occupancy: u64,
    total_cycles: u32,
}

impl LVStreamingServer {
    pub fn new(
        bind_addr: &str,
        target_addr: &str,
        fps: u32,
        screen_no: usize,
        width: u32,
        height: u32,
        bitrate: u32,
        quit_rx: Receiver<bool>,
        bitrate_mtx: Arc<Mutex<u32>>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            bind_addr: bind_addr.to_owned(),
            target_addr: target_addr.to_owned(),
            fps,
            screen_no,
            width,
            height,
            quit_rx,
            old_bitrate: bitrate,
            bitrate_mtx,
            udp_fd: None,
            // Statistics stuff
            total_queue_occupancy: 0,
            total_cycles: 0,
        })
    }

    pub fn bytes_in_send_queue(&self) -> Result<u32, Box<dyn std::error::Error>> {
        // info!("udp fd set to {:?}", self.udp_fd);
        match self.udp_fd {
            Some(fd) => unsafe {
                let mut data = 0;
                tiocoutq(fd, &mut data)?;
                Ok(data)
            },
            None => Ok(0),
        }
    }

    pub fn begin(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // HUGE lag because frame backlog exists if this is like anything more than 2
        let (frame_push, frame_recv) = flume::bounded(2);
        self.start_capture_thread(frame_push)?;
        self.start_send_loop(frame_recv)?;
        Ok(())
    }

    pub fn start_capture_thread(
        &self,
        frame_push: Sender<ImageBuffer<Rgb<u8>, Vec<u8>>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let sixty_fps = Duration::new(0, (1000000000. / self.fps as f32) as u32);
        let screen = *Screen::all()?
            .get(self.screen_no)
            .expect("Expected a screen");

        thread::spawn(move || {
            let mut capturer = LVLinuxCapturer::new(screen).expect("Could not start capturer");
            loop {
                match capturer.capture() {
                    Ok(frame) => {
                        // Throw the stuff into the mpmc
                        match frame_push.try_send(frame) {
                            // This is normal.
                            Err(e) => trace!("could not push to q {:?}", e),
                            _ => {}
                        }
                    }
                    Err(e) => {
                        error!("captured frame was None! {:#?}", e);
                    }
                }

                spin_sleep::sleep(sixty_fps);
            }
        });

        Ok(())
    }

    pub fn start_send_loop(
        &mut self,
        frame_recv: Receiver<ImageBuffer<Rgb<u8>, Vec<u8>>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        info!("bind addr {}", self.bind_addr);
        let socket = UdpSocket::bind(&self.bind_addr).expect("Failed to make socket");
        let encoder =
            encoder::default_encoder(self.width, self.height, self.old_bitrate, self.fps as f32)
                .expect("Failed to make encoder");
        let mut packager = LVPackager::new(encoder, self.fps).expect("Failed to make packager");
        let mut rtp_pkt = BytesMut::new();

        LVStatisticsCollector::register_data("server_packet_sending", LVDataType::TimeSeries);
        LVStatisticsCollector::register_data("server_bitrate_queue_occupancy", LVDataType::XYData);

        info!("server bound to {}", self.bind_addr);

        let timer = Instant::now();
        self.udp_fd = Some(socket.as_raw_fd());

        // TODO: Add statistics
        loop {
            self.total_queue_occupancy += self.bytes_in_send_queue()? as u64;
            self.total_cycles += 1;

            match self.quit_rx.try_recv() {
                Ok(val) if val => {
                    info!("Ctrl-c received, statistics logged, quitting...");
                    break;
                }
                Err(e) => {
                    if e != TryRecvError::Empty {
                        error!("quit_rx from statistics module gave {:?}", e)
                    }
                }
                _ => warn!("quit_rx gave false value!"),
            }

            match frame_recv.recv() {
                Ok(frame) => {
                    match packager.process_frame(frame, timer.elapsed().as_millis() as u64) {
                        Ok(_) => {}
                        Err(e) => error!("process_frame returned {:?}", e),
                    }
                }
                Err(e) => error!("frame_recv returned {:?}", e),
            }

            // Update the bitrate if the mutex has a new bitrate value.
            {
                let possibly_new_bitrate =
                    self.bitrate_mtx.lock().expect("Failed to lock bitrate mtx");
                if self.old_bitrate != *possibly_new_bitrate {
                    debug!("Updating bitrate to {}", *possibly_new_bitrate);

                    LVStatisticsCollector::update_data(
                        "server_bitrate_queue_occupancy",
                        LVDataPoint::XYValue((
                            self.old_bitrate as f32,
                            (self.total_queue_occupancy as f64 / self.total_cycles as f64) as f32,
                        )),
                    );

                    // Reset statistics
                    self.total_queue_occupancy = 0;
                    self.total_cycles = 0;

                    match packager.update_bitrate(*possibly_new_bitrate) {
                        Ok(ook) => debug!("updated bitrate"),
                        Err(e) => error!("Failed to set bitrate with {:?}", e),
                    }
                    self.old_bitrate = *possibly_new_bitrate;
                }
            }

            let loop_pkg = Instant::now();
            while packager.has_rtp() {
                match packager.send_next_pkt(&socket, &self.target_addr) {
                    Ok(bytes) => debug!("sent {} bytes to addr", bytes),
                    Err(e) => error!("send_to returned {:?}", e),
                }
            }
            LVStatisticsCollector::update_data(
                "server_packet_sending",
                LVDataPoint::TimeElapsed(loop_pkg.elapsed()),
            );
        }

        Ok(())
    }
}
