use std::mem::{align_of, size_of};

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

pub fn max_align() -> usize {
    *[
        align_of::<LVKeyboardEvent>(),
        align_of::<LVKeyboardEvent>(),
        align_of::<LVMouseClickEvent>(),
        align_of::<LVMouseWheelEvent>(),
        align_of::<LVMouseMoveEvent>(),
    ]
    .iter()
    .max()
    .unwrap()
}

pub fn input_packet_size() -> usize {
    // The input packet is 1 (for the variant and struct padding) plus the size of the largest structure.
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
        // The 1 byte is included in here.
        + max_align()
}

// Right now, these u8s corresond to the KeyCode and ElementState enums in winit respectively.
#[repr(C)]
#[derive(bytemuck::NoUninit, bytemuck::AnyBitPattern, Clone, Copy, Default, Debug)]
pub struct LVKeyboardEvent {
    pub key_code: u8,
    pub state: u8,
}

impl LVKeyboardEvent {
    pub fn new(key_code: KeyCode, state: ElementState) -> Self {
        Self {
            key_code: unsafe { std::mem::transmute(key_code) },
            state: match state {
                ElementState::Pressed => 0,
                ElementState::Released => 1,
            },
        }
    }
    pub fn get_key_code(&self) -> KeyCode {
        // instead of typing out a huge table, we will use unsafe for now
        // TODO fix

        unsafe { std::mem::transmute(self.key_code) }
    }

    pub fn get_element_state(&self) -> Option<ElementState> {
        match self.state {
            0 => Some(ElementState::Pressed),
            1 => Some(ElementState::Released),
            // TODO don't panic
            _ => None,
        }
    }
}

// Right now, these u8s corresond to the MouseButton event in winit.
#[repr(C)]
#[derive(bytemuck::NoUninit, bytemuck::AnyBitPattern, Clone, Copy, Default, Debug)]
pub struct LVMouseClickEvent {
    pub button: u32,
}

impl LVMouseClickEvent {
    pub fn new(btn: MouseButton) -> Self {
        Self {
            button: match btn {
                MouseButton::Left => 0,
                MouseButton::Right => 1,
                MouseButton::Middle => 2,
                MouseButton::Back => 3,
                MouseButton::Forward => 4,
                MouseButton::Other(_) => 5,
            },
        }
    }
    pub fn get_button(&self) -> Option<MouseButton> {
        // TODO fix
        match self.button {
            0 => Some(MouseButton::Left),
            1 => Some(MouseButton::Right),
            2 => Some(MouseButton::Middle),
            3 => Some(MouseButton::Back),
            4 => Some(MouseButton::Forward),
            _ => None,
        }
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
