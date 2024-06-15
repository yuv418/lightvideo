use std::mem::size_of;

use int_enum::IntEnum;
pub use winit::{
    event::{ElementState, MouseButton},
    keyboard::KeyCode,
};

// We have a separate enum because we need
// to have the variant and data broken up for network
// serialization.
#[derive(Debug)]
pub enum LVInputEvent {
    KeyboardEvent(LVKeyboardEvent),
    MouseClickEvent(LVMouseClickEvent),
    MouseWheelEvent(LVMouseWheelEvent),
    MouseMoveEvent(LVMouseMoveEvent),
}

#[repr(u8)]
#[derive(Debug, PartialEq, IntEnum)]
pub enum LVInputEventType {
    KeyboardEvent = 0,
    MouseClickEvent = 1,
    MouseWheelEvent = 2,
    MouseMoveEvent = 3,
}

pub fn input_packet_size() -> usize {
    // The input packet is 1 (for the variant) plus the size of the largest structure.
    // Smaller structures are padded.
    [
        size_of::<LVKeyboardEvent>(),
        size_of::<LVKeyboardEvent>(),
        size_of::<LVMouseClickEvent>(),
        size_of::<LVMouseWheelEvent>(),
        size_of::<LVMouseMoveEvent>(),
    ]
    .iter()
    .max()
    .unwrap()
        + 1
}

// Right now, these u8s corresond to the KeyCode and ElementState enums in winit respectively.
#[repr(C)]
#[derive(bytemuck::NoUninit, bytemuck::AnyBitPattern, Clone, Copy, Default, Debug)]
pub struct LVKeyboardEvent {
    key_code: u8,
    state: u8,
}

impl LVKeyboardEvent {
    pub fn get_key_code(&self) -> KeyCode {
        // instead of typing out a huge table, we will use unsafe for now
        // TODO fix

        unsafe { std::mem::transmute(self.key_code) }
    }
    pub fn get_element_state(&self) -> ElementState {
        // TODO fix
        unsafe { std::mem::transmute(self.key_code) }
    }
}

// Right now, these u8s corresond to the MouseButton event in winit.
#[repr(C)]
#[derive(bytemuck::NoUninit, bytemuck::AnyBitPattern, Clone, Copy, Default, Debug)]
pub struct LVMouseClickEvent {
    button: u32,
}

impl LVMouseClickEvent {
    pub fn get_button(&self) -> MouseButton {
        // TODO fix
        unsafe { std::mem::transmute(self.button) }
    }
}

// We're not going to bother with this right now.
#[repr(C)]
#[derive(bytemuck::NoUninit, bytemuck::AnyBitPattern, Clone, Copy, Default, Debug)]
pub struct LVMouseWheelEvent {}

#[repr(C)]
#[derive(bytemuck::NoUninit, bytemuck::AnyBitPattern, Clone, Copy, Default, Debug)]
pub struct LVMouseMoveEvent {
    pub x: f64,
    pub y: f64,
}
