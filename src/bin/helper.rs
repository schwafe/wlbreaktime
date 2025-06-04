use core::str;
use std::io::ErrorKind;
use std::os::unix::net::UnixDatagram;

use std::{env, fs};
const SOCKET_NAME: &str = "wlbreaktime.socket";
const HELPER_SOCKET_NAME: &str = "wlbreaktime-helper.socket";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let runtime_dir = env::var("XDG_RUNTIME_DIR")?;

    let mut args = env::args();
    // TODO: provide a description of possible arguments
    if args.len() < 2 {
        println!("No arguments provided!");
        return Ok(());
    } else if args.len() > 3 {
        println!("Too many arguments!");
        return Ok(());
    }
    let arg = args.nth(1).unwrap();
    let mut minutes = None;

    match arg.as_str() {
        "set" => minutes = Some(args.nth(2).expect("no duration to set to provided!")),
        "break" | "reset" | "time" | "skip" => {
            assert!(args.nth(2).is_none(), "did not expect a second argument!");
        }
        _ => {
            println!(
                "Incorrect first argument! Please provide one of the following arguments: break|set|reset|time|skip"
            );
            return Ok(());
        }
    }

    let result = UnixDatagram::bind(runtime_dir.clone() + "/" + HELPER_SOCKET_NAME);
    let socket;

    match result {
        Err(err) if err.kind() == ErrorKind::AddrInUse => {
            // the helper probably crashed the last time it ran and the socket is still linked, so
            // it needs to be unlinked before trying again
            fs::remove_file(runtime_dir.clone() + "/" + HELPER_SOCKET_NAME)?;
            socket = UnixDatagram::bind(runtime_dir.clone() + "/" + HELPER_SOCKET_NAME)
                .expect("Unable to bind socket even on second attempt!");
        }
        Err(err) => {
            let kind = err.kind();
            panic!("Unable to bind socket because of error '{err:?}' with ErrorKind '{kind}'!");
        }
        Ok(s) => socket = s,
    }

    let result = socket.send_to(arg.as_bytes(), runtime_dir.clone() + "/" + SOCKET_NAME);

    match result {
        Err(err) if err.kind() == ErrorKind::NotFound => {
            panic!("Breaktimer does not seem to be running!"); // socket is not available
        }
        Err(err) => panic!("Error '{err}' unexpectedly occured while sending a message!"),
        Ok(_) => {
            // everything is fine, do nothing
        }
    }

    match arg.as_str() {
        "set" => {
            socket.send_to(
                minutes.unwrap().as_bytes(),
                runtime_dir.clone() + "/" + SOCKET_NAME,
            )?;
        }
        "time" => {
            let mut buffer = [0; 30];
            let bytes_read = socket.recv(&mut buffer)?;
            let string_read = str::from_utf8(&buffer[..bytes_read])?;

            println!("{string_read} seconds remain until the next break!");
        }
        _ => {
            // no action needed
        }
    }

    fs::remove_file(runtime_dir + "/" + HELPER_SOCKET_NAME)?; // unlink socket
    Ok(())
}
