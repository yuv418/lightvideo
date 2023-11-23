use anyhow::anyhow;
use core::slice;
use image::Rgb;
use libc::{IPC_CREAT, IPC_PRIVATE, IPC_RMID};
use log::{debug, error};
use screenshots::Screen;
use std::{
    os::raw::{c_char, c_ulong},
    time::Instant,
};
use x11_dl::{
    xlib::{self, Display, XImage, Xlib, ZPixmap},
    xshm::{self, XShmSegmentInfo, Xext},
};

use xcb::{
    shm::{Attach, GetImage, Seg},
    x::{Drawable, ImageFormat, ImageOrder},
    Connection,
};

use super::LVCapturer;

pub struct LVLinuxCapturer {
    conn: Connection,
    xlib: Xlib,
    xshm: Xext,
    dsp: *mut Display,
    x_image: *mut XImage,
    bgr_buffer: *mut u8,
    bgr_buffer_len: usize,
    x: i32,
    y: i32,
    root_window: c_ulong,
    all_planes: u32,
    pub width: u16,
    pub height: u16,
}

impl LVLinuxCapturer {
    pub fn new(screen: Screen) -> Result<Self, Box<dyn std::error::Error>> {
        let (conn, index) =
            xcb::Connection::connect_with_extensions(None, &[xcb::Extension::Shm], &[])?;

        let width = (screen.display_info.width as f32 * screen.display_info.scale_factor) as u16;
        let height = (screen.display_info.height as f32 * screen.display_info.scale_factor) as u16;

        let buffer_size = width as usize * height as usize * 4 as usize;

        let (xlib, xshm, dsp, x_image, bgr_buffer, root_window, all_planes) = unsafe {
            let xlib = xlib::Xlib::open().unwrap();
            let xshm = xshm::Xext::open().unwrap();

            let dsp = (xlib.XOpenDisplay)(std::ptr::null());

            if (xshm.XShmQueryExtension)(dsp) != 1 {
                error!("XShmQueryExtension returns false!");
                return Err(anyhow!("XShmQueryExtension returns false!").into());
            }

            let x_screen = (xlib.XDefaultScreen)(dsp);

            // Setup shared memory
            let shm_id = libc::shmget(IPC_PRIVATE, buffer_size, IPC_CREAT | 0o600);
            debug!("shm_id is {}", shm_id);
            // Map into process address space
            let bgr_buffer = libc::shmat(shm_id, std::ptr::null(), 0) as *mut u8;
            debug!("bgr_buffer is {:p}", bgr_buffer);
            if bgr_buffer == std::ptr::null_mut() {
                libc::perror(std::ptr::null());
                return Err(anyhow!("could not map shared memory!").into());
            }

            // Make sure that the shm is deallocated if the program crashes
            libc::shmctl(shm_id, IPC_RMID, std::ptr::null_mut());

            let shm_segment_info = XShmSegmentInfo {
                // Null/unset
                shmseg: 0,
                shmid: shm_id,
                shmaddr: bgr_buffer as *mut i8,
                readOnly: 0,
            };

            // Allocate image from XCreateImage
            let x_image = (xshm.XShmCreateImage)(
                dsp,
                (xlib.XDefaultVisual)(dsp, x_screen),
                (xlib.XDefaultDepth)(dsp, x_screen) as u32,
                ZPixmap,
                std::ptr::null_mut(),
                &shm_segment_info as *const XShmSegmentInfo as *mut XShmSegmentInfo,
                0,
                0,
            );
            debug!("ximage is {:p}", x_image);
            let mut x_image_mod = x_image.as_mut().unwrap();
            x_image_mod.data = bgr_buffer as *mut i8;
            x_image_mod.width = width as i32;
            x_image_mod.height = height as i32;

            // Attach shm into x
            (xshm.XShmAttach)(
                dsp,
                &shm_segment_info as *const XShmSegmentInfo as *mut XShmSegmentInfo,
            );
            (xlib.XSync)(dsp, 0);
            debug!("xshmattach and xsync are done.");

            // Get root window and all planes
            let root_window = (xlib.XDefaultRootWindow)(dsp);
            let all_planes = (xlib.XAllPlanes)() as u32;

            (
                xlib,
                xshm,
                dsp,
                x_image,
                bgr_buffer,
                root_window,
                all_planes,
            )
        };
        Ok(Self {
            xlib,
            xshm,
            dsp,
            conn,
            x_image,
            x: screen.display_info.x,
            y: screen.display_info.y,
            bgr_buffer,
            bgr_buffer_len: buffer_size,
            width,
            height,
            root_window,
            all_planes,
        })
    }
}

// TODO: https://stackoverflow.com/questions/34176795/any-efficient-way-of-converting-ximage-data-to-pixel-map-e-g-array-of-rgb-quad
// TODO: use XShm for image buffer
impl LVCapturer for LVLinuxCapturer {
    // Adapted from https://github.com/nashaofu/screenshots-rs/blob/master/src/linux/xorg.rs
    fn capture(
        &mut self,
    ) -> Result<image::ImageBuffer<Rgb<u8>, Vec<u8>>, Box<dyn std::error::Error>> {
        // I would really like to offload this screen to be elsewhere. It's a waste to do this every time.

        debug!("bgr_buffer is {:p}", self.bgr_buffer);
        debug!("self.x {} self.y {}", self.x, self.y);
        let time = Instant::now();
        unsafe {
            debug!("x_image is {:?}", *self.x_image);
            (self.xshm.XShmGetImage)(
                self.dsp,
                self.root_window,
                self.x_image,
                self.x,
                self.y,
                self.all_planes,
            );
        }
        debug!("XShmGetImage elapsed {:.4?}", time.elapsed());

        let vec = {
            let bts = unsafe { slice::from_raw_parts(self.bgr_buffer, self.bgr_buffer_len) };
            let mut v = vec![];
            v.extend_from_slice(bts);
            v
        };
        /*for y in 0..height {
            for x in 0..width {
                let dst_start_index = (((y * width) + x) * 3) as usize;
                let src_start_index = (((y * width) + x) * 4) as usize;
                // debug!("bgra vec capacity is {}", self.bgra_vec.capacity());
                // alpha
                self.bgra_vec[dst_start_index + 3] = 255;

                // extract_bgra
                if depth == 24 {
                    if self.bit_order == ImageOrder::LsbFirst {
                        // debug!("native representation is bgr");
                        // how 2 make faster?
                        self.bgra_vec[src_start_index] = bytes[dst_start_index];
                        self.bgra_vec[src_start_index + 1] = bytes[dst_start_index + 1];
                        self.bgra_vec[src_start_index + 2] = bytes[dst_start_index + 2];
                    } else {
                        // debug!("native representation is rgb");
                        // how 2 make faster?
                        self.bgra_vec[src_start_index] = bytes[dst_start_index + 2];
                        self.bgra_vec[src_start_index + 1] = bytes[dst_start_index + 1];
                        self.bgra_vec[src_start_index + 2] = bytes[dst_start_index];
                    }
                } else {
                    unimplemented!()
                }
            }
        }*/

        Ok(
            image::ImageBuffer::from_vec(self.width as u32, self.height as u32, vec)
                .ok_or(anyhow!("Does not fit in imgbuf"))?,
        )
    }
}
