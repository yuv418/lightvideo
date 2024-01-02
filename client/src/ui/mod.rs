use std::sync::Arc;

use log::{error, info};
use winit::{
    dpi::{LogicalSize, PhysicalSize, Size},
    event::*,
    event_loop::{ControlFlow, EventLoop},
    keyboard::{Key, NamedKey},
    window::{Window, WindowBuilder},
};

mod wgpu_state;

use wgpu_state::WGPUState;

use crate::double_buffer::{self, DoubleBuffer};

pub struct VideoUI {}

impl VideoUI {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {})
    }

    pub async fn run(
        &self,
        double_buffer: Arc<DoubleBuffer>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let eloop = EventLoop::new()?;
        let window = WindowBuilder::new()
            .with_inner_size(Size::Physical(PhysicalSize {
                width: 1920,
                height: 1088,
            }))
            .with_resizable(false)
            .build(&eloop)?;

        let mut state = WGPUState::new(window, double_buffer).await;

        eloop.run(move |event, elwt| match event {
            Event::WindowEvent { ref event, .. } => {
                if !state.input(event) {
                    match event {
                        WindowEvent::Resized(physical_size) => {
                            state.resize(*physical_size);
                        }
                        // How to update this for winit 0.29?
                        /*WindowEvent::ScaleFactorChanged { inner_size_writer, .. } => {
                            inner_size_writer.request_inner_size(new_inner_size)
                            state.resize(**new_inner_size);
                        }*/
                        WindowEvent::CloseRequested
                        | WindowEvent::KeyboardInput {
                            event:
                                KeyEvent {
                                    state: ElementState::Pressed,
                                    logical_key: Key::Named(NamedKey::Escape),
                                    ..
                                },
                            ..
                        } => {
                            info!("window close requested");
                            elwt.exit()
                        }
                        WindowEvent::RedrawRequested => {
                            state.update();
                            match state.render() {
                                Ok(_) => {}
                                Err(wgpu::SurfaceError::OutOfMemory) => {
                                    error!("wgpu surface out of memory");
                                    elwt.exit();
                                }
                                Err(e) => error!("state.render has error {:?}", e),
                            }
                        }
                        _ => {}
                    }
                }
            }
            Event::AboutToWait => state.window().request_redraw(),
            _ => {}
        })?;

        Ok(())
    }
}
