use super::LVInputEmulator;
use anyhow::anyhow;
use log::{debug, info, warn};
use net::input::{ElementState, LVInputEvent, MouseButton};
use winit::platform::scancode::PhysicalKeyExtScancode;
use xcb::{xtest::FakeInput, Connection};

pub struct LVX11InputEmulator {
    conn: Connection,
    fake_input: FakeInput,
}

impl LVX11InputEmulator {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let (conn, index) =
            xcb::Connection::connect_with_extensions(None, &[xcb::Extension::Shm], &[])?;

        let setup = conn.get_setup();
        let root = setup
            .roots()
            .nth(0)
            .ok_or_else(|| anyhow!("Could not find a screen."))?
            .root();

        let fake_input = FakeInput {
            r#type: 0,
            detail: 0,
            time: 0,
            root,
            root_x: 0,
            root_y: 0,
            deviceid: 0,
        };

        Ok(Self { conn, fake_input })
    }
}

impl LVInputEmulator for LVX11InputEmulator {
    fn write_event(&mut self, ev: net::input::LVInputEvent) -> Result<(), anyhow::Error> {
        match ev {
            LVInputEvent::KeyboardEvent(kb_ev) => {
                self.fake_input.r#type = match kb_ev.get_element_state() {
                    Some(ElementState::Pressed) => x11::xlib::KeyPress as u8,
                    Some(ElementState::Released) => x11::xlib::KeyRelease as u8,
                    None => {
                        warn!("got invalid element state None");
                        return Err(anyhow!("Other mouse button received"));
                    }
                };

                self.fake_input.detail = kb_ev
                    .get_key_code()
                    .to_scancode()
                    .ok_or_else(|| anyhow!("Could not convert keycode to scancode."))?
                    .try_into()?
            }
            LVInputEvent::MouseClickEvent(click_ev) => {
                // left is 1, middle 2, right 3, guessing back is 8, forward is 9
                self.fake_input.detail = match click_ev.get_button() {
                    Some(MouseButton::Left) => 1,
                    Some(MouseButton::Right) => 2,
                    Some(MouseButton::Middle) => 3,
                    Some(MouseButton::Back) => 8,
                    Some(MouseButton::Forward) => 9,
                    _ => {
                        warn!("received other mouse button");
                        return Err(anyhow!("Other mouse button received"));
                    }
                }
            }
            LVInputEvent::MouseWheelEvent(wheel_ev) => {
                unimplemented!()
            }
            LVInputEvent::MouseMoveEvent(move_ev) => {
                // Set to true makes it absolute
                self.fake_input.detail = 1;
                self.fake_input.root_x = move_ev.x as i16;
                self.fake_input.root_y = move_ev.y as i16;
            }
        }

        self.fake_input.time = x11::xlib::CurrentTime as u32;

        // We don't bother checking this request. Maybe we should.
        let _ = self.conn.send_request(&(self.fake_input));

        Ok(())
    }
}
