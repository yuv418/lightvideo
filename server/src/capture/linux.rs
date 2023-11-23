use anyhow::anyhow;
use image::{Rgb};
use log::debug;
use screenshots::Screen;
use xcb::{
    x::{Drawable, GetImage, ImageFormat, ImageOrder},
    Connection,
};

use super::LVCapturer;

pub struct LVLinuxCapturer {
    screen: Screen,
    conn: Connection,
    index: i32,
    get_image: GetImage,
    bit_order: ImageOrder,
    bgr_vec: Vec<u8>,
}

impl LVLinuxCapturer {
    pub fn new(screen: Screen) -> Result<Self, Box<dyn std::error::Error>> {
        let (conn, index) = xcb::Connection::connect(None)?;

        let width = (screen.display_info.width as f32 * screen.display_info.scale_factor) as u16;
        let height = (screen.display_info.height as f32 * screen.display_info.scale_factor) as u16;

        let (get_image, bit_order) = {
            let setup = conn.get_setup();
            let x_screen = setup
                .roots()
                .nth(index as usize)
                .ok_or_else(|| anyhow!("Could not find a screen."))?;
            (
                GetImage {
                    format: ImageFormat::ZPixmap,
                    drawable: Drawable::Window(x_screen.root()),
                    x: (screen.display_info.x as f32 * screen.display_info.scale_factor) as i16,
                    y: (screen.display_info.y as f32 * screen.display_info.scale_factor) as i16,
                    width,
                    height,

                    plane_mask: u32::MAX,
                },
                setup.bitmap_format_bit_order(),
            )
        };

        Ok(Self {
            screen,
            conn,
            bit_order,
            index,
            get_image,
            // Times for 4 bgra data
            bgr_vec: vec![0; width as usize * height as usize * 4 as usize],
        })
    }
}

// TODO: https://stackoverflow.com/questions/34176795/any-efficient-way-of-converting-ximage-data-to-pixel-map-e-g-array-of-rgb-quad
// TODO: use XShm for image buffer
impl LVCapturer for LVLinuxCapturer {
    // Adapted from https://github.com/nashaofu/screenshots-rs/blob/master/src/linux/xorg.rs
    fn capture(
        &mut self,
    ) -> Result<image::ImageBuffer<Rgb<u8>, &[u8]>, Box<dyn std::error::Error>> {
        // I would really like to offload this screen to be elsewhere. It's a waste to do this every time.

        let get_image_cookie = self.conn.send_request(&(self.get_image));
        let get_image_reply = self.conn.wait_for_reply(get_image_cookie)?;
        let bytes = get_image_reply.data();
        let depth = get_image_reply.depth();

        let _width =
            (self.screen.display_info.width as f32 * self.screen.display_info.scale_factor) as u32;
        let _height =
            (self.screen.display_info.height as f32 * self.screen.display_info.scale_factor) as u32;

        if self.bit_order == ImageOrder::LsbFirst {
            debug!("bytes len {}", bytes.len());
            debug!("bgr_vec capacity {}", self.bgr_vec.capacity());
            self.bgr_vec.clone_from_slice(bytes);
        }

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

        debug!("depth is {}", depth);

        Ok(image::ImageBuffer::from_raw(
            self.screen.display_info.width,
            self.screen.display_info.height,
            self.bgr_vec.as_slice(),
        )
        .ok_or(anyhow!("Does not fit in imgbuf"))?)
    }
}
