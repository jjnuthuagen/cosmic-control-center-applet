//! List the globals advertised by $WAYLAND_DISPLAY. Demo tooling.
use wayland_client::{protocol::wl_registry, Connection, Dispatch, QueueHandle};

struct App;
impl Dispatch<wl_registry::WlRegistry, ()> for App {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global { interface, version, .. } = event {
            println!("{interface} v{version}");
        }
    }
}

fn main() {
    let conn = Connection::connect_to_env().expect("connect to wayland");
    let display = conn.display();
    let mut q = conn.new_event_queue();
    let qh = q.handle();
    display.get_registry(&qh, ());
    q.roundtrip(&mut App).expect("roundtrip");
}
