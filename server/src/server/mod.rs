use bytes::{BufMut, BytesMut};
use flume::{Receiver, Sender};
use image::{ImageBuffer, Rgb};
use log::{debug, error, info};
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
    encoder::LVEncoder,
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
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            bind_addr: bind_addr.to_owned(),
            target_addr: target_addr.to_owned(),
            fps,
            screen_no,
            width,
            height,
            bitrate,
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
                            Err(e) => error!("could not push to q {:?}", e),
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
        let encoder = LVEncoder::new(self.width, self.height, self.bitrate, self.fps as f32)
            .expect("Failed to make encoder");
        let mut packager = LVPackager::new(encoder, self.fps).expect("Failed to make packager");
        let mut rtp_pkt = BytesMut::new();

        LVStatisticsCollector::register_data("server_packet_sending", LVDataType::TimeSeries);

        info!("server bound to {}", self.bind_addr);

        let timer = Instant::now();

        // TODO: Add statistics
        loop {
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
            while let Some(rtp) = packager.pop_rtp() {
                rtp_pkt.resize(rtp.marshal_size(), 0);
                debug!("rtp_pkt capacity {}", rtp_pkt.capacity());
                debug!("rtp_pkt len {}", rtp_pkt.len());
                debug!(
                    "rtp_pkt remaining_mut before {} and marshal_size {}",
                    (&mut rtp_pkt).remaining_mut(),
                    rtp.marshal_size()
                );
                debug!("rtp_pkt remaining_mut after {}", rtp_pkt.remaining_mut());
                rtp.marshal_to(&mut rtp_pkt)?;
                match socket.send_to(&rtp_pkt, &self.target_addr) {
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
