use anyhow::anyhow;
use core::slice;
use image::Rgb;
use libc::{IPC_CREAT, IPC_PRIVATE, IPC_RMID};
use log::debug;
use screenshots::Screen;
use std::{os::raw::c_char, time::Instant};
use xcb::{
    shm::{Attach, GetImage, Seg},
    x::{Drawable, ImageFormat, ImageOrder},
    Connection,
};

use super::LVCapturer;

pub struct LVLinuxCapturer {
    conn: Connection,
    get_image: GetImage,
    bit_order: ImageOrder,
    bgr_buffer: *mut u8,
    bgr_buffer_len: usize,
}

impl LVLinuxCapturer {
    pub fn new(screen: Screen) -> Result<Self, Box<dyn std::error::Error>> {
        let (conn, index) =
            xcb::Connection::connect_with_extensions(None, &[xcb::Extension::Shm], &[])?;

        let width = (screen.display_info.width as f32 * screen.display_info.scale_factor) as u16;
        let height = (screen.display_info.height as f32 * screen.display_info.scale_factor) as u16;

        let buffer_size = width as usize * height as usize * 4 as usize;

        // Setup shared memory segment
        let (bgr_buffer, seg) = unsafe {
            let shm_id = libc::shmget(IPC_PRIVATE, buffer_size, IPC_CREAT | 0o600) as u32;
            debug!("shm_id is {}", shm_id);
            // Map into process address space
            let bgr_buffer = libc::shmat(shm_id as i32, std::ptr::null(), 0) as *mut u8;
            debug!("bgr_buffer is {:p}", bgr_buffer);
            if bgr_buffer == std::ptr::null_mut() {
                libc::perror(std::ptr::null());
                return Err(anyhow!("failed to open shared memory buffer").into());
            }

            // Make sure that the shm is deallocated if the program crashes
            libc::shmctl(shm_id as i32, IPC_RMID, std::ptr::null_mut());

            let seg: Seg = conn.generate_id();
            let void_cookie = conn.send_request_checked(&Attach {
                shmseg: seg,
                shmid: shm_id,
                read_only: false,
            });
            conn.check_request(void_cookie)?;

            (bgr_buffer, seg)
        };

        let (get_image, bit_order) = {
            let setup = conn.get_setup();
            let x_screen = setup
                .roots()
                .nth(index as usize)
                .ok_or_else(|| anyhow!("Could not find a screen."))?;
            (
                GetImage {
                    drawable: Drawable::Window(x_screen.root()),
                    x: (screen.display_info.x as f32 * screen.display_info.scale_factor) as i16,
                    y: (screen.display_info.y as f32 * screen.display_info.scale_factor) as i16,
                    width,
                    height,
                    plane_mask: u32::MAX,
                    format: ImageFormat::ZPixmap as u8, // ZPixmap
                    shmseg: seg,
                    offset: 0,
                },
                setup.bitmap_format_bit_order(),
            )
        };

        Ok(Self {
            conn,
            bit_order,
            get_image,
            bgr_buffer,
            bgr_buffer_len: buffer_size,
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

        let get_image_cookie = self.conn.send_request(&(self.get_image));
        let time = Instant::now();
        let _ = self.conn.wait_for_reply(get_image_cookie)?;
        let bytes = unsafe { slice::from_raw_parts(self.bgr_buffer, self.bgr_buffer_len) };
        debug!("XShmGetImage took {:.4?}", time.elapsed());

        if self.bit_order == ImageOrder::LsbFirst {
            debug!("bytes len {}", bytes.len());
        } else {
            unimplemented!("RGBA not implemented");
        }

        Ok(image::ImageBuffer::from_vec(
            self.get_image.width.into(),
            self.get_image.height.into(),
            bytes.to_vec(),
        )
        .ok_or(anyhow!("Does not fit in imgbuf"))?)
    }
}
