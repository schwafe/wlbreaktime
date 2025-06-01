use env_logger::{Builder, Env};
use log::info;
use std::{thread::sleep, time::Duration};
use wayland_client::{Connection, EventQueue};

mod wayland;
use wayland::{State, check_for_globals, show_popup};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    Builder::from_env(Env::default().default_filter_or("info")).init();

    // wayland set-up
    let connection = Connection::connect_to_env().unwrap();
    let display = connection.display();
    let mut event_queue: EventQueue<State> = connection.new_event_queue();
    let qh = event_queue.handle();
    let _registry = display.get_registry(&qh, ());

    let mut data = State {
        wl_shm: None,
        surface_size: None,
        accepted_formats: Vec::new(),
        compositor: None,
        base: None,
    };

    // waiting on compositor to advertise globals
    event_queue.blocking_dispatch(&mut data).unwrap();

    // make sure all necessary globals have been bound
    check_for_globals(&data)?;

    loop {
        //waiting until it's break time
        sleep(Duration::from_secs(15)); // TODO: make configurable
        info!("Slept for 300 seconds");
        show_popup(&mut event_queue, &mut data, &qh)?;
    }
}
