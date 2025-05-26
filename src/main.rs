use env_logger::{Builder, Env};
use log::{error, info};
use std::{
    fs::{self, File},
    io::{BufWriter, Write},
    os::fd::AsFd,
    thread::sleep,
    time::Duration,
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

#[derive(Debug)]
struct SurfaceSize {
    width: i32,
    height: i32,
}

#[derive(Debug)]
pub struct State {
    // pub(crate) wl_seat: Option<wl_seat::WlSeat>,
    // pub(crate) qh: QueueHandle<State>,
    // pub(crate) dbus_handlers: CallbackListHandle,
    // pub(crate) tx: mpsc::Sender<Request>,
    // pub(crate) lua: LuaHandle,
    // pub(crate) base: XdgWmBase,
    wl_shm: Option<wl_shm::WlShm>,
    surface_size: Option<SurfaceSize>,
    accepted_formats: Vec<WEnum<Format>>,
}

lazy_static::lazy_static! {
    pub static ref COMPOSITOR: std::sync::Mutex<Option<wl_compositor::WlCompositor>> = std::sync::Mutex::new(None);
    pub static ref WL_SURFACE: std::sync::Mutex<Option<wl_surface::WlSurface>> = std::sync::Mutex::new(None);
    pub static ref XDG_WM_BASE: std::sync::Mutex<Option<xdg_wm_base::XdgWmBase>> = std::sync::Mutex::new(None);
    pub static ref XDG_SURFACE: std::sync::Mutex<Option<xdg_surface::XdgSurface>> = std::sync::Mutex::new(None);
    pub static ref XDG_TOP: std::sync::Mutex<Option<xdg_toplevel::XdgToplevel>> = std::sync::Mutex::new(None);
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
        state: &mut Self,
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
                    let compositor =
                        registry.bind::<wl_compositor::WlCompositor, _, _>(name, 1, qh, ());

                    let surface = compositor.create_surface(qh, ());
                    *WL_SURFACE.lock().unwrap() = Some(surface);
                    info!("Created surface!");

                    *COMPOSITOR.lock().unwrap() = Some(compositor);
                }
                "wl_shm" => {
                    state.wl_shm = Some(registry.bind(name, version, qh, ()));
                    info!("Bound WlShm");
                }
                "xdg_wm_base" => {
                    let base = registry.bind::<xdg_wm_base::XdgWmBase, _, _>(name, 1, qh, ());

                    let surface = WL_SURFACE.lock().unwrap();
                    if let Some(surface) = surface.as_ref() {
                        let xdg_surface = base.get_xdg_surface(surface, qh, ());
                        info!("Created xdg_surface!");

                        let xdg_top = xdg_surface.get_toplevel(qh, ());
                        info!("Created xdg_top!");
                        xdg_top.set_title("Title".to_string());
                        xdg_top.set_app_id("Breaktimer ID".to_string());
                        xdg_top.set_fullscreen(None);

                        *XDG_TOP.lock().unwrap() = Some(xdg_top);
                        *XDG_SURFACE.lock().unwrap() = Some(xdg_surface);

                        surface.commit();
                        info!("Performed initial commit on surface!");
                    } else {
                        error!(
                            "Unable to create an xdg_surface, because no wl_surface was created before this!"
                        );
                    }

                    *XDG_WM_BASE.lock().unwrap() = Some(base);
                }
                "wl_output" => {
                    // let wl_output = registry.bind::<wl_output::WlOutput, _, _>(name, 1, qh, ());
                    // TODO: is bind to output relevant?
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
        _: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        _: &(),
        _: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let xdg_surface = XDG_SURFACE.lock().unwrap();
        if let Some(xdg_surface) = xdg_surface.as_ref() {
            match event {
                xdg_surface::Event::Configure { serial } => {
                    // using the provided XdgSurface handle seems like it might be able to create racing
                    // conditions

                    // TODO: since this event is handled after the others, the configuring should
                    // already be done?
                    xdg_surface.ack_configure(serial);
                    info!("Acked configure event");

                    if state.accepted_formats.len() > 0 {
                        // let buffer: wl_buffer::WlBuffer = TODO: attach buffer
                        // TODO: commit? at least normally after a configure event a commit is needed
                    } else {
                        error!("The compositor did not advertise any buffer formats it accepts.")
                    }
                }
                _ => {
                    error!("Received an xdg-surface event {event:?} that isn't handled yet!");
                }
            };
        } else {
            error!("Received an event for a non-existent xdg-surface (or lost the surface...)");
        }
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
                states: _, // TODO: states are probably important
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

fn show_popup(
    event_queue: &mut EventQueue<State>,
    data: &mut State,
    compositor: &wl_compositor::WlCompositor,
    base: &xdg_wm_base::XdgWmBase,
    qh: &QueueHandle<State>,
) -> Result<(), Box<dyn std::error::Error>> {
    let surface = compositor.create_surface(&qh, ());

    let xdg_surface = base.get_xdg_surface(&surface, &qh, ());
    info!("Created xdg_surface!");

    let xdg_top = xdg_surface.get_toplevel(&qh, ());
    info!("Created xdg_top!");
    xdg_top.set_title("Title".to_string());
    xdg_top.set_app_id("Breaktimer ID".to_string());
    xdg_top.set_fullscreen(None);

    surface.commit();
    info!("Performed initial commit on surface!");
    // waiting on compositor to react and then acking the configure event
    event_queue.blocking_dispatch(data)?;

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
    format: &Format,
) -> Result<(), Box<dyn std::error::Error>> {
    let result = File::create_new(filename);
    match result {
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
        Err(_) => Ok(()), //Err(result), TODO: how to match the AlreadyExists error? and how to
                          //return the other errors?
    }
}

// The main function of our program
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
    };

    // waiting on compositor to advertise globals
    event_queue.blocking_dispatch(&mut data).unwrap();

    {
        let c = COMPOSITOR.lock().unwrap();
        let compositor = c.as_ref().unwrap();
        let b = XDG_WM_BASE.lock().unwrap();
        let base = b.as_ref().unwrap();
        show_popup(&mut event_queue, &mut data, &compositor, &base, &qh)?;
    }

    let wl_shm = data.wl_shm
            .as_ref()
            .expect("No wl_shm was bound even though all globals should have been advertised ages ago and it is needed to create a pool!");

    let surface_size = data.surface_size.as_ref().unwrap();
    let format = choose_format(&data.accepted_formats);
    let stride = surface_size.width * 4; // always choosing a format of 32 bits

    // TODO: 1. find a good place for the file
    // 2. using a file seems very inefficient. Can I get a file descriptor of RAM storage?
    let filename = "../screens/pool-".to_string()
        + &surface_size.width.to_string()
        + "-"
        + &surface_size.height.to_string()
        + "-Xrgb8888"; // TODO: how to get format.to_string()?

    let pool_size = surface_size.height * stride * 2; // TODO: * 2 because of double-buffering?

    draw_checker_board(&filename, surface_size, &format)?;
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(&filename)
        .unwrap();

    let pool = wl_shm.create_pool(file.as_fd(), pool_size, &qh, ());

    let buffer = pool.create_buffer(
        0,
        surface_size.width,
        surface_size.height,
        stride,
        format,
        &qh,
        (),
    );
    info!("Created buffer!");

    {
        let s = WL_SURFACE.lock().unwrap();
        let surface = s
            .as_ref()
            .expect("No surface even though a buffer was already created for it?!");
        surface.attach(Some(&buffer), 0, 0);
        // surface.damage_buffer(0, 0, i32::MAX, i32::MAX); // TODO: is damage_buffer recommended on the first commit?
        surface.commit();
        info!("Attached buffer to surface and committed surface");
    }

    event_queue.blocking_dispatch(&mut data).unwrap();
    sleep(Duration::from_secs(1));
    info!("Slept for one second!");

    {
        let t = XDG_TOP.lock().unwrap();
        let top = t
            .as_ref()
            .expect("XDG_TOP was lost?? by now the XdgToplevel should have been created for sure!");
        top.unset_fullscreen();
        info!("Unset fullscreen for XdgToplevel");
    }

    // expecting a configure event
    event_queue.blocking_dispatch(&mut data).unwrap();
    {
        let s = WL_SURFACE.lock().unwrap();
        let surface = s.as_ref().unwrap();
        surface.commit();
        info!("Committed after minimizing!");
    }

    sleep(Duration::from_secs(1));
    info!("Slept for one second!");

    info!("Shutting down!");

    pool.destroy(); // "A buffer will keep a reference to the pool it was created from so it is valid to destroy the pool immediately after creating a buffer from it."
    info!("Destroyed pool!");

    buffer.destroy();
    info!("Destroyed buffer!");
    {
        let xdg_top = XDG_TOP.lock().unwrap();
        if let Some(xdg_top) = xdg_top.as_ref() {
            xdg_top.destroy();
            info!("Destroyed xdg_top!");
        }
    }
    {
        let xdg_surface = XDG_SURFACE.lock().unwrap();
        if let Some(xdg_surface) = xdg_surface.as_ref() {
            xdg_surface.destroy();
            info!("Destroyed xdg_surface!");
        }
    }
    {
        let surface = WL_SURFACE.lock().unwrap();
        if let Some(surface) = surface.as_ref() {
            surface.destroy();
            info!("Destroyed surface!");
        }
    }

    Ok(())
}
