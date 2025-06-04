use core::str;
use libsystemd::{
    activation::{self, FileDescriptor, IsType},
    daemon::{self, NotifyState},
};
use std::{
    io::ErrorKind,
    os::{
        fd::{FromRawFd, IntoRawFd},
        unix::net::UnixDatagram,
    },
    time::{Duration, Instant},
};
use wayland_client::{Connection, EventQueue};

mod wayland;
use wayland::{State, check_for_globals, show_popup};

// TODO: make configurable
const BREAK_DURATION_SECONDS: u64 = 80;
const SECONDS_BETWEEN_BREAKS: u64 = 1800;

fn wait_until_break(socket: &mut UnixDatagram) -> Result<(), Box<dyn std::error::Error>> {
    //waiting until it's break time
    println!("Work time!");
    let mut breaktime = false;
    let mut now = Instant::now();

    while !breaktime {
        // setting read timeout every time, because for every break it's set to a different value
        // and on interrupts it needs to be adjusted
        socket.set_read_timeout(Some(Duration::from_secs(
            SECONDS_BETWEEN_BREAKS - now.elapsed().as_secs(),
        )))?;

        let mut buffer = [0; 300];
        let result = socket.recv_from(&mut buffer);
        match result {
            Ok((bytes_read, return_address)) => {
                assert!(bytes_read > 0);
                // trimming the last byte, because it's one of the zeros written by us
                let string_read = str::from_utf8(&buffer[..bytes_read])?;

                match string_read {
                    "break" => {
                        println!("Skipped to break!");
                        breaktime = true;
                    }
                    "reset" => {
                        println!("Reset timer, next break in {SECONDS_BETWEEN_BREAKS} seconds!");
                        now = Instant::now();
                    }
                    "time" => {
                        let remainder = SECONDS_BETWEEN_BREAKS - now.elapsed().as_secs();
                        match return_address.as_pathname() {
                            Some(path) => {
                                println!(
                                    "Responding to inquiry about remaining time before break! {remainder} seconds remain."
                                );
                                socket.send_to(remainder.to_string().as_bytes(), path)?;
                            }
                            None => {
                                println!(
                                    "Unable to respond to inquiry about time, because it came from an unbound socket!"
                                );
                            }
                        }
                    }
                    _ => println!("[work]: Received unknown argument '{string_read}'"),
                }
            }
            Err(err) if err.kind() == ErrorKind::WouldBlock => {} // do nothing on interrupt
            Err(err) => {
                let kind = err.kind();
                panic!("[work]: Unexpected error '{err}' with ErrorKind {kind} reading!");
            }
        }

        if now.elapsed().as_secs() >= SECONDS_BETWEEN_BREAKS {
            println!("Work time is over!");
            breaktime = true;
        }
    }

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if !daemon::booted() {
        println!("Not running systemd, early exit.");
        return Ok(());
    };

    // receiving the socket from systemd
    let mut descriptors = activation::receive_descriptors(true)?; // TODO: true or false?
    assert!(descriptors.len() == 1);

    let fd = descriptors.pop().unwrap();
    assert!(
        fd.is_unix(),
        "The systemd socket was configured incorrectly!"
    );

    let mut socket = unsafe { UnixDatagram::from_raw_fd(FileDescriptor::into_raw_fd(fd)) };

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

    // breaktimer is ready -> notify systemd
    let sent = daemon::notify(true, &[NotifyState::Ready]).expect("notify failed");
    assert!(
        sent,
        "The systemd service seems to have been configured incorrectly (not Type=notify)!"
    );

    loop {
        wait_until_break(&mut socket)?;

        show_popup(&mut event_queue, &mut data, &qh, &mut socket)?;
    }
}
