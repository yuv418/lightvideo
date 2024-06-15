use net::input::LVInputEvent;

pub mod x11;

pub trait LVInputEmulator: Send {
    fn write_event(&mut self, ev: LVInputEvent) -> Result<(), anyhow::Error>;
}
