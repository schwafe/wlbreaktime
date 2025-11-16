// TODO posting errors to journald at an incredibly fast rate: "an error occurred on output stream: A backend-specific error has occurred: ALSA function
// 'snd_pcm_poll_descriptors_revents' failed with error 'Unknown errno (-5)'"
use core::str;
use libsystemd::{
    activation::{self, FileDescriptor, IsType},
    daemon::{self, NotifyState},
};
use std::{
    io::{Cursor, ErrorKind},
    os::{
        fd::{FromRawFd, IntoRawFd},
        unix::net::UnixDatagram,
    },
    process::Command,
    sync::Arc,
    time::{Duration, Instant},
};
// show pop-up
use wayland_client::{Connection, EventQueue};
// play a sound
use rodio::{Decoder, OutputStream, OutputStreamHandle, source::Source};
// show notifications
use notify_rust::Notification;

mod wayland;
use wayland::{State, check_for_globals, show_popup};

use crate::wayland::wait_until_work;

mod config;

const NORMAL_READ_TIMEOUT: u64 = 3;

/*
 * returns true if work time was skipped
 */
fn wait_until_break(
    socket: &mut UnixDatagram,
    break_interval: u64,
) -> Result<bool, Box<dyn std::error::Error>> {
    //waiting until it's break time
    println!("Work time!");
    let mut breaktime = false;
    let mut now = Instant::now();
    let mut skipped = false;

    // to enable changing the remaining time, the break duration needs to be mutable
    let mut work_duration_seconds = break_interval;

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
                        skipped = true;
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
                        work_duration_seconds = break_interval;
                        now = Instant::now();
                        socket.send_to(work_duration_seconds.to_string().as_bytes(), path)?;
                        println!("Reset timer, next break in {work_duration_seconds} seconds!");
                    }
                    "get" => {
                        let remainder = work_duration_seconds
                            .checked_sub(now.elapsed().as_secs())
                            .unwrap_or(0);

                        socket.send_to(remainder.to_string().as_bytes(), path)?;
                        // TODO implement some way (here and in wayland.rs) for the helper to know
                        // when it's break time and when it's work time, e.g. not just sending the
                        // seconds but also a 0/1 signal
                    }
                    &_ => panic!("found match, but non-optional capture group is missing!"),
                }
            }
            Err(err) if err.kind() == ErrorKind::WouldBlock => {} // do nothing on timeout
            Err(err) if err.kind() == ErrorKind::Interrupted => {
                // interrupt happens when system wakes up from suspension -> treat like reset
                work_duration_seconds = break_interval;
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

    Ok(skipped)
}

fn play_sound(
    stream_handle: &OutputStreamHandle,
    sound_data: &Arc<[u8]>,
) -> Result<(), Box<dyn std::error::Error>> {
    // https://stackoverflow.com/questions/78742705/how-to-play-sound-from-memory-using-rodio
    let source = Decoder::new(Cursor::new(Arc::clone(&sound_data))).unwrap();

    // Play the sound directly on the device
    stream_handle.play_raw(source.convert_samples())?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if !daemon::booted() {
        println!("Not running systemd, early exit.");
        return Ok(());
    };

    // systemd setup -- receive file descriptor (socket handle)
    let mut descriptors = activation::receive_descriptors(true)?;
    assert!(
        descriptors.len() == 1,
        "Systemd passed more than one file descriptor (socket). Configuration must be wrong!"
    );

    let fd = descriptors.pop().unwrap();
    assert!(
        fd.is_unix(),
        "The systemd socket was configured incorrectly!"
    );

    let mut socket = unsafe { UnixDatagram::from_raw_fd(FileDescriptor::into_raw_fd(fd)) };

    let config = config::load_configuration()?;

    // audio setup
    // get output stream handle to default physical sound device
    let (_stream, stream_handle) = OutputStream::try_default().unwrap();
    // load sound into memory and create a pointer to it
    let bytes = include_bytes!("../resources/rebana_l_gong.wav");
    let sound_data: Arc<[u8]> = Arc::from(bytes.clone());

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

    // breaktime is ready -> notify systemd
    let sent = daemon::notify(true, &[NotifyState::Ready]).expect("notify failed");
    assert!(
        sent,
        "The systemd service seems to have been configured incorrectly (not Type=notify)!"
    );

    loop {
        let skipped = wait_until_break(&mut socket, config.break_interval)?;

        if !skipped && config.show_notification {
            Notification::new()
                .summary("It's break time!")
                .body("The next break starts in 10 seconds.")
                .show()?;
            std::thread::sleep(Duration::from_secs(10));
        }

        if config.play_sound {
            play_sound(&stream_handle, &sound_data)?;
        }

        if config.turn_off_monitors {
            let status = Command::new("niri")
                .arg("msg")
                .arg("action")
                .arg("power-off-monitors")
                .status();

            if let Err(err) = status {
                println!("Monitors could not be turned off! The error: {err}");
            }
        }

        if config.show_popup {
            show_popup(
                &mut event_queue,
                &mut data,
                &qh,
                &mut socket,
                config.break_duration,
            )?;
        } else {
            wait_until_work(&mut socket, config.break_duration)?;
        }

        if config.turn_off_monitors {
            let status = Command::new("niri")
                .arg("msg")
                .arg("action")
                .arg("power-on-monitors")
                .status();

            if let Err(err) = status {
                println!("Monitors could not be turned on! The error: {err}");
            }
        }

        if config.play_sound {
            play_sound(&stream_handle, &sound_data)?;
        }
    }
}
