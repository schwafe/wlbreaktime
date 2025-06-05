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

const BREAK_DURATION_SECONDS: u64 = 80;
const SECONDS_BETWEEN_BREAKS: u64 = 1800;
const NORMAL_READ_TIMEOUT: u64 = 3;

fn wait_until_break(socket: &mut UnixDatagram) -> Result<(), Box<dyn std::error::Error>> {
    //waiting until it's break time
    println!("Work time!");
    let mut breaktime = false;
    let mut now = Instant::now();

    // to enable changing the remaining time, the break duration needs to be mutable
    let mut work_duration_seconds = SECONDS_BETWEEN_BREAKS;

    while !breaktime {
        // setting read timeout every time, because for every break it's set to a different value
        // and on interrupts it needs to be adjusted
        let seconds_until_break = work_duration_seconds
            .checked_sub(now.elapsed().as_secs())
            .unwrap_or(1);

        socket.set_read_timeout(Some(Duration::from_secs(seconds_until_break)))?;

        let mut buffer = [0; 300];
        let result = socket.recv_from(&mut buffer);
        match result {
            Ok((bytes_read, return_address)) => {
                assert!(bytes_read > 0);
                // not every command needs a response, however it simplifies things if
                // unbound sockets are not accepted
                let path = return_address
                    .as_pathname()
                    .expect("Unable to respond, because the message came from an unbound socket!");
                // trimming the last byte, because it's one of the zeros written by us
                let string_read = str::from_utf8(&buffer[..bytes_read])?;
                match string_read {
                    "break" => {
                        println!("Skipped to break!");
                        breaktime = true;
                    }
                    "set" => {
                        socket.set_read_timeout(Some(Duration::from_secs(NORMAL_READ_TIMEOUT)))?;
                        buffer = [0; 300];
                        let result = socket.recv_from(&mut buffer);
                        match result {
                            Ok((bytes_read, _)) => {
                                let string_read = str::from_utf8(&buffer[..bytes_read])?;
                                let minutes = string_read.parse::<u64>().unwrap();
                                work_duration_seconds = minutes * 60;
                                now = Instant::now();
                                println!(
                                    "Set timer, next break in {work_duration_seconds} seconds!"
                                );
                            }
                            Err(err) if err.kind() == ErrorKind::WouldBlock => println!(
                                "While trying to read the second argument (minutes), a timeout happened and no time could be set! Probably the helper crashed."
                            ),
                            Err(err) => {
                                let kind = err.kind();
                                panic!(
                                    "[work]: Unexpected error '{err}' with ErrorKind {kind} while trying to read second argument (minutes)!"
                                );
                            }
                        }
                    }
                    "reset" => {
                        work_duration_seconds = SECONDS_BETWEEN_BREAKS;
                        now = Instant::now();
                        socket.send_to(work_duration_seconds.to_string().as_bytes(), path)?;
                        println!("Reset timer, next break in {work_duration_seconds} seconds!");
                    }
                    "time" => {
                        let remainder = work_duration_seconds
                            .checked_sub(now.elapsed().as_secs())
                            .unwrap_or(0);

                        println!(
                            "Responding to inquiry about remaining time before break! {remainder} seconds remain."
                        );
                        socket.send_to(remainder.to_string().as_bytes(), path)?;
                    }
                    &_ => panic!("found match, but non-optional capture group is missing!"),
                }
            }
            Err(err) if err.kind() == ErrorKind::WouldBlock => {} // do nothing on timeout
            Err(err) if err.kind() == ErrorKind::Interrupted => {
                // interrupt happens e.g. when system wakes up from suspension -> treat like reset
                work_duration_seconds = SECONDS_BETWEEN_BREAKS;
                now = Instant::now();
                println!(
                    "Reset timer because system suspension was detected. Next break is in {work_duration_seconds} seconds!"
                );
            }
            Err(err) => {
                let kind = err.kind();
                panic!("[work]: Unexpected error '{err}' with ErrorKind {kind} reading!");
            }
        }

        if now.elapsed().as_secs() >= work_duration_seconds {
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
    let mut descriptors = activation::receive_descriptors(true)?;
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
