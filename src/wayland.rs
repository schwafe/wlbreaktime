use core::str;
use log::{error, info};
use std::{
    env,
    fs::{self, File},
    io::{BufWriter, ErrorKind, Write},
    os::{fd::AsFd, unix::net::UnixDatagram},
    time::{Duration, Instant},
};

use wayland_client::{
    Connection, Dispatch, EventQueue, QueueHandle, WEnum,
    protocol::{
        wl_buffer, wl_compositor, wl_output,
        wl_registry::{Event, WlRegistry},
        wl_shm::{self, Format},
        wl_shm_pool,
        wl_surface::{self},
    },
};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};

use crate::BREAK_DURATION_SECONDS;

#[derive(Debug)]
pub(crate) struct SurfaceSize {
    width: i32,
    height: i32,
}

#[derive(Debug)]
pub(crate) struct State {
    pub(crate) wl_shm: Option<wl_shm::WlShm>,
    pub(crate) surface_size: Option<SurfaceSize>,
    pub(crate) accepted_formats: Vec<WEnum<Format>>,
    pub(crate) compositor: Option<wl_compositor::WlCompositor>,
    pub(crate) base: Option<xdg_wm_base::XdgWmBase>,
}

impl Dispatch<wl_output::WlOutput, ()> for State {
    fn event(
        _state: &mut Self,
        _output: &wl_output::WlOutput,
        event: wl_output::Event,
        _: &(),
        _: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if let wl_output::Event::Geometry {
            x,
            y,
            physical_width,
            physical_height,
            subpixel,
            make,
            model,
            transform,
        } = event
        {
            info!(
                "Output geometry: x: {}, y: {}, physical_width: {}, physical_height: {}, subpixel: {:?}, make: {}, model: {}, transform: {:?}",
                x, y, physical_width, physical_height, subpixel, make, model, transform
            );
        }
    }
}

impl Dispatch<WlRegistry, ()> for State {
    fn event(
        data: &mut Self,
        registry: &WlRegistry,
        event: Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<State>,
    ) {
        // When receiving events from the wl_registry, we are only interested in the
        // `global` event, which signals a new available global.
        // When receiving this event, we just print its characteristics in this example.
        if let Event::Global {
            name,
            interface,
            version,
        } = event
        {
            // info!("[{}] {} (v{})", name, interface, version);
            match &interface[..] {
                "wl_compositor" => {
                    data.compositor =
                        Some(registry.bind::<wl_compositor::WlCompositor, _, _>(name, 1, qh, ()));
                    info!("Bound compositor");
                }
                "wl_shm" => {
                    data.wl_shm = Some(registry.bind(name, version, qh, ()));
                    info!("Bound WlShm");
                }
                "xdg_wm_base" => {
                    data.base =
                        Some(registry.bind::<xdg_wm_base::XdgWmBase, _, _>(name, 1, qh, ()));
                    info!("Bound base");
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<wl_buffer::WlBuffer, ()> for State {
    fn event(
        _: &mut Self,
        _: &wl_buffer::WlBuffer,
        event: wl_buffer::Event,
        _: &(),
        _: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        info!("Buffer event {event:?}");
    }
}

impl Dispatch<wl_compositor::WlCompositor, ()> for State {
    fn event(
        _: &mut Self,
        _: &wl_compositor::WlCompositor,
        _: wl_compositor::Event,
        _: &(),
        _: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        info!("Compositor event");
    }
}

impl Dispatch<wl_shm::WlShm, ()> for State {
    fn event(
        state: &mut Self,
        _: &wl_shm::WlShm,
        event: wl_shm::Event,
        _: &(),
        _: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_shm::Event::Format { format } => {
                info!("This compositor supports the {format:?} format");
                state.accepted_formats.push(format);
            }
            _ => {
                error!("Unconfigured wlShm event {event:?}")
            }
        }
    }
}

impl Dispatch<wl_shm_pool::WlShmPool, ()> for State {
    fn event(
        _: &mut Self,
        _: &wl_shm_pool::WlShmPool,
        event: wl_shm_pool::Event,
        _: &(),
        _: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        info!("WlShm_pool event {event:?}");
    }
}

impl Dispatch<wl_surface::WlSurface, ()> for State {
    fn event(
        _: &mut Self,
        _: &wl_surface::WlSurface,
        event: wl_surface::Event,
        _: &(),
        _: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        info!("wl_surface event {event:?}");
    }
}

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for State {
    fn event(
        _: &mut Self,
        base: &xdg_wm_base::XdgWmBase,
        event: xdg_wm_base::Event,
        _: &(),
        _: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            info!("Received pong with serial {serial}");
            base.pong(serial);
        } else {
            error!("Unexpected XdgWmBase event {event:?}");
        }
    }
}

impl Dispatch<xdg_surface::XdgSurface, ()> for State {
    fn event(
        state: &mut Self,
        xdg_surface: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        _: &(),
        _: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            xdg_surface::Event::Configure { serial } => {
                // using the provided XdgSurface handle seems like it might be able to create racing
                // conditions

                xdg_surface.ack_configure(serial);
                info!("Acked configure event");

                if state.accepted_formats.is_empty() {
                    panic!("The compositor did not advertise any buffer formats it accepts.")
                }
            }
            _ => {
                error!("Received an xdg-surface event {event:?} that isn't handled yet!");
            }
        };
    }
}

impl Dispatch<xdg_toplevel::XdgToplevel, ()> for State {
    fn event(
        state: &mut Self,
        _: &xdg_toplevel::XdgToplevel,
        event: xdg_toplevel::Event,
        _: &(),
        _: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            xdg_toplevel::Event::Configure {
                width,
                height,
                states: _,
            } => {
                state.surface_size = Some(SurfaceSize { width, height });
                info!("XdgToplevel configure event to width {width} and height {height}");
            }
            _ => {
                info!("Unconfigured XdgToplevel event {event:?}");
            }
        }
    }
}

fn wait_until_work(socket: &mut UnixDatagram) -> Result<(), Box<dyn std::error::Error>> {
    // waiting until the break is over
    println!("Break time!");
    let mut breaktime = true;
    let now = Instant::now();
    // setting read timeout every time, because outside of every break it's set to a different value
    socket.set_read_timeout(Some(Duration::from_secs(BREAK_DURATION_SECONDS)))?;

    while breaktime {
        let mut buffer = [0; 300];
        let result = socket.recv(&mut buffer);
        match result {
            Ok(bytes_read) => {
                assert!(bytes_read > 0);
                // trimming the last byte, because it's one of the zeros written by us
                let string_read = str::from_utf8(&buffer[..bytes_read])?;

                if string_read == "skip" {
                    println!("Break was skipped!");
                    breaktime = false;
                } else {
                    println!("[break]: Received unknown argument '{string_read}'");
                }
            }
            Err(err) if err.kind() == ErrorKind::WouldBlock => {
                let elapsed = now.elapsed().as_secs();
                if elapsed < BREAK_DURATION_SECONDS {
                    println!("[break]: Read was interrupted after {elapsed} seconds.");
                    socket.set_read_timeout(Some(Duration::from_secs(
                        BREAK_DURATION_SECONDS - elapsed,
                    )))?;
                    breaktime = true;
                } else {
                    println!("Break is over!");
                    breaktime = false;
                }
            }
            Err(err) => {
                let kind = err.kind();
                panic!("[break]: Unexpected error '{err}' with ErrorKind {kind} reading!");
            }
        }
    }

    Ok(())
}

pub(crate) fn show_popup(
    event_queue: &mut EventQueue<State>,
    data: &mut State,
    qh: &QueueHandle<State>,
    socket: &mut UnixDatagram,
) -> Result<(), Box<dyn std::error::Error>> {
    let wl_surface = data.compositor.as_ref().unwrap().create_surface(&qh, ());

    let xdg_surface = data
        .base
        .as_ref()
        .unwrap()
        .get_xdg_surface(&wl_surface, &qh, ());

    let xdg_top = xdg_surface.get_toplevel(&qh, ());
    xdg_top.set_title("Title".to_string());
    xdg_top.set_app_id("Breaktimer ID".to_string());
    xdg_top.set_fullscreen(None);

    // performing initial commit
    wl_surface.commit();
    // waiting on compositor to react and then acking the configure event
    event_queue.blocking_dispatch(data)?;

    // TODO: creating a pool only needs to be done once, so long as the surface size does not
    // change -> don't destroy the pool, but instead keep the reference and reuse it
    let surface_size = data.surface_size.as_ref().unwrap_or(&SurfaceSize {
        height: 1080,
        width: 1920,
    });
    // FIXME: sometimes the surface size is missing
    // .expect("Surface size was not provided!");
    let format = choose_format(&data.accepted_formats);
    let stride = surface_size.width * 4; // always choosing a format of 32 bits

    // TODO: using a file seems inefficient. Can I get a file descriptor of RAM storage?
    let runtime_dir = env::var("XDG_RUNTIME_DIR")?;
    let filename = runtime_dir
        + "/wlbreaktime-pool-"
        + &surface_size.width.to_string()
        + "-"
        + &surface_size.height.to_string()
        // + "-Xrgb8888"; // TODO: how to get format.to_string()?
        + &format!("{format:?}"); // HACK: depending on the Debug trait does not sound good
    //
    // TODO: * 2 because of double-buffering necessary?
    let pool_size = surface_size.height * stride * 2;

    draw_checker_board(&filename, surface_size, &format)?;
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(&filename)
        .unwrap();

    let pool = data
        .wl_shm
        .as_ref()
        .unwrap()
        .create_pool(file.as_fd(), pool_size, &qh, ());

    let buffer = pool.create_buffer(
        0,
        surface_size.width,
        surface_size.height,
        stride,
        format,
        &qh,
        (),
    );
    info!("Created pool, buffer, xdg_top, xdg_surface and wl_surface!");

    wl_surface.attach(Some(&buffer), 0, 0);
    wl_surface.commit();

    event_queue.blocking_dispatch(data).unwrap();

    wait_until_work(socket)?;

    pool.destroy(); // "A buffer will keep a reference to the pool it was created from so it is valid to destroy the pool immediately after creating a buffer from it."
    buffer.destroy();
    xdg_top.destroy();
    xdg_surface.destroy();
    wl_surface.destroy();
    info!("Destroyed pool, buffer, xdg_top, xdg_surface and wl_surface!");

    event_queue.flush()?;
    Ok(())
}

fn choose_format(formats: &Vec<WEnum<Format>>) -> Format {
    if formats.contains(&WEnum::Value(Format::Xrgb8888)) {
        return Format::Xrgb8888;
    } else if formats.contains(&WEnum::Value(Format::Argb8888)) {
        return Format::Argb8888;
    } else {
        error!("Neither Xrgb8888 nor Argb8888 are supported");
        return Format::Xbgr8888;
    }
}

fn draw_checker_board(
    filename: &str,
    surface_size: &SurfaceSize,
    _format: &Format, // TODO: use format to determine what's written
) -> Result<(), Box<dyn std::error::Error>> {
    let result = File::create_new(filename);
    match result {
        Err(err) if err.kind() == ErrorKind::AlreadyExists => {
            // do nothing, because the file has already been generated
            Ok(())
        }
        Ok(file) => {
            let mut buf = BufWriter::new(file);
            let mut index = 0;
            while index < surface_size.height * surface_size.width {
                if index % 2 == 0 {
                    buf.write(b"FF666666")?;
                } else {
                    buf.write(b"FFEEEEEE")?;
                }
                index += 1;
            }

            // TODO: empty part for double-buffering?
            index = 0;
            while index < surface_size.height * surface_size.width {
                buf.write(b"00000000")?;
                index += 1;
            }
            Ok(())
        }
        Err(err) => {
            let kind = err.kind();
            panic!(
                "Error while trying to create the wayland pool file. Error '{err:?}' with ErrorKind '{kind}'"
            );
        }
    }
}

pub(crate) fn check_for_globals(data: &State) -> Result<(), &'static str> {
    if data.compositor.is_none() {
        return Err("no compositor");
    }
    if data.base.is_none() {
        return Err("no base");
    }
    if data.wl_shm.is_none() {
        return Err("no wl_shm");
    }

    Ok(())
}
