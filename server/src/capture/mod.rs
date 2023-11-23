pub mod linux;

use image::{Rgb};

pub trait LVCapturer {
    fn capture(&mut self)
        -> Result<image::ImageBuffer<Rgb<u8>, &[u8]>, Box<dyn std::error::Error>>;
}
