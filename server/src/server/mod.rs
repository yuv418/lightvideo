use bytes::{BufMut, BytesMut};
use flume::{Receiver, Sender, TryRecvError};
use image::{ImageBuffer, Rgb};
use log::{debug, error, info, trace, warn};
use screenshots::Screen;
use statistics::{
    collector::LVStatisticsCollector,
    statistics::{LVDataPoint, LVDataType},
};
use std::{
    net::UdpSocket,
    thread,
    time::{Duration, Instant},
};
use webrtc_util::{Marshal, MarshalSize};

use crate::{
    capture::{linux::LVLinuxCapturer, LVCapturer},
    encoder,
    packager::LVPackager,
};

pub struct Server {
    bind_addr: String,
    target_addr: String,
    fps: u32,
    screen_no: usize,
    width: u32,
    height: u32,
    bitrate: u32,
    quit_rx: Receiver<bool>,
}

impl Server {
    pub fn new(
        bind_addr: &str,
        target_addr: &str,
        fps: u32,
        screen_no: usize,
        width: u32,
        height: u32,
        bitrate: u32,
        quit_rx: Receiver<bool>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            bind_addr: bind_addr.to_owned(),
            target_addr: target_addr.to_owned(),
            fps,
            screen_no,
            width,
            height,
            bitrate,
            quit_rx,
        })
    }

    pub fn begin(&self) -> Result<(), Box<dyn std::error::Error>> {
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
        &self,
        frame_recv: Receiver<ImageBuffer<Rgb<u8>, Vec<u8>>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        info!("bind addr {}", self.bind_addr);
        let socket = UdpSocket::bind(&self.bind_addr).expect("Failed to make socket");
        let encoder =
            encoder::default_encoder(self.width, self.height, self.bitrate, self.fps as f32)
                .expect("Failed to make encoder");
        let mut packager = LVPackager::new(encoder, self.fps).expect("Failed to make packager");
        let mut rtp_pkt = BytesMut::new();

        LVStatisticsCollector::register_data("server_packet_sending", LVDataType::TimeSeries);

        info!("server bound to {}", self.bind_addr);

        let timer = Instant::now();

        // TODO: Add statistics
        loop {
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
