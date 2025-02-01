use std::{
    collections::HashMap,
    error::Error,
    fmt::Display,
    io::{self, BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, Condvar, Mutex},
};

use crate::{
    file_utils,
    packet_utils::{self, PacketError, ServerError, WallpaperPacket},
};

pub struct WallpaperServer {
    directory: Arc<Mutex<String>>,
    wallpaper: Arc<Mutex<String>>,
    main_trigger: Arc<(Mutex<bool>, Condvar)>,
    duration: u64,
    address: String,
}

impl WallpaperServer {
    /// Initializes a `WallpaperServer` instance with a backgrounds directory. The server can then be started with `.start()`
    pub fn new(directory: String, duration: u64, address: String) -> Result<Self, Box<dyn Error>> {
        let wallpapers = file_utils::reload_directory(&directory)?;

        let first_wallpaper = wallpapers.get(0).unwrap_or(&String::new()).clone();

        Ok(WallpaperServer {
            duration,
            directory: Arc::new(Mutex::new(directory)),
            wallpaper: Arc::new(Mutex::new(first_wallpaper)),
            main_trigger: Arc::new((Mutex::new(false), Condvar::new())),
            address,
        })
    }

    /// Starts the Wallpaper TCP server.
    ///
    /// If the server is terminated with a `Stop` via TCP, this function will return `Ok(())`.
    ///
    /// If the server encounters a critical error, it will quit and propagate it by returning an `Err(_)`.
    pub fn start(&mut self) -> Result<(), Box<dyn Error>> {
        // Initialize TCP Listener
        let listener = TcpListener::bind(&self.address)?;

        log::info!("Starting server at {}", &self.address);

        log::info!("Wallpaper will cycle every {} seconds", &self.duration);
        // Set up Atomic Mutexes for the child thread to use
        let child_trigger = self.main_trigger.clone();
        let child_wallpaper = self.wallpaper.clone();
        let child_directory = self.directory.clone();
        let mut index = 0;
        let duration = self.duration;

        // Spawn the child thread. This thread will be responsible for cycling the wallpaper every DURATION seconds
        std::thread::spawn(move || loop {
            match cycle_wallpapers(
                &child_trigger,
                &child_wallpaper,
                &child_directory,
                &mut index,
                duration,
            ) {
                Ok(_) => {}
                Err(e) => log::error!("Ran into error: {e}"),
            }
        });

        // Start listening for requests on the TCP socket!
        for stream in listener.incoming() {
            let stream = match stream {
                Ok(stream) => {
                    self.handle_stream(stream);
                }
                Err(e) => {
                    log::error!("Ran into an error when handling request: {e}");
                    continue;
                }
            };
        }
        Ok(())
    }

    /// Reads the raw request from TCP bytestream, decodes the packet, and submits the request to be processed.
    fn handle_stream(&mut self, mut stream: TcpStream) {
        // Read bytes into the buffer using a reader
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let Ok(buffer) = packet_utils::extract_bytes_buffered(&mut reader) else {
            log::error!("Error while attempting to read from TCP stream");
            packet_utils::send_empty_response(&stream);
            return;
        };
        log::info!("Request received");

        let packet = match packet_utils::decode_packet(buffer) {
            Ok(packet) => {
                log::info!(
                    "Successfully decoded packet!
Headers: {:#?}
Body: {:?}
                ",
                    &packet.headers,
                    &packet.body
                );
                packet
            }
            Err(e) => {
                log::warn!("Ran into error while decoding packet: {e}");
                packet_utils::send_empty_response(&stream);
                return;
            }
        };

        // If the WallpaperControl header is not present, discard packet
        if !packet.headers.contains_key("WallpaperControl") {
            log::warn!("Necessary packet header not found.");
            packet_utils::send_empty_response(&stream);
            return;
        }

        let (status, response) = match self.process_packet(packet) {
            Ok((status, body)) => (status, packet_utils::build_response(status, Some(body))),
            Err(e) => (400, packet_utils::build_response(400, Some(format!("{e}")))),
        };

        log::info!("Replying with packet:\n{response}");
        match stream.write_all(response.as_bytes()) {
            Ok(_) => {}
            Err(e) => {
                log::error!("Error when attempting to write to TCP stream: {e}");
                return;
            }
        }

        // I hereby declare that HTTP status code 269 means the connection was successfully closed
        if status == 269 {
            std::process::exit(0);
        }
    }

    /// Processes a decoded packet's request.
    fn process_packet<'a>(
        &mut self,
        packet: WallpaperPacket,
    ) -> Result<(u64, String), Box<dyn Error + '_>> {
        let command = packet.headers["WallpaperControl"].clone();
        let response: (u64, String) = match command.as_str() {
            "Update" => {
                log::info!("Received Update request");
                *self.wallpaper.lock().unwrap() = packet.body;
                let (lock, cvar) = &*self.main_trigger;

                let mut trigger = lock.lock().unwrap();
                *trigger = true;
                cvar.notify_one();

                (200, "OK".to_string())
            }
            "Cycle" => {
                log::info!("Received Cycle request");
                let (lock, cvar) = &*self.main_trigger;

                let mut trigger = lock.lock().unwrap();
                *trigger = true;
                cvar.notify_one();

                (200, "OK".to_string())
            }
            "Stop" => {
                log::info!("Received Stop request, closing server...");
                (269, "Stopping server...".to_string())
            }
            "GetDir" => {
                log::info!("Received GetDir request");
                (200, self.directory.lock().unwrap().clone())
            }
            "SetDir" => {
                log::info!("Received SetDir request");

                match file_utils::reload_directory(&packet.body.trim()) {
                    Ok(_) => {
                        *self.directory.lock().unwrap() = packet.body;
                        (200, "OK".to_string())
                    }
                    Err(e) => (
                        400,
                        format!("ERROR: The directory provided could not be set: {e}"),
                    ),
                }
            }
            _ => {
                return Err(Box::new(PacketError(
                    "Received unknown request, discarding packet...",
                )))
            }
        };

        Ok(response)
    }
}

/// Ran by the child thread to periodically cycle wallpapers
///
/// Internally increments `index`.
fn cycle_wallpapers(
    child_trigger: &Arc<(Mutex<bool>, Condvar)>,
    child_wallpaper: &Arc<Mutex<String>>,
    child_directory: &Arc<Mutex<String>>,
    index: &mut usize,
    duration: u64,
) -> Result<(), Box<dyn Error>> {
    let (lock, cvar) = &**child_trigger;

    // Wait for trigger or timeout
    let mut triggered = lock.lock().unwrap();
    let _ = cvar.wait_timeout(triggered, std::time::Duration::from_secs(duration));

    // Read next wallpaper
    let current_wallpaper = child_wallpaper.lock().unwrap().clone();

    // Reload directory
    let directory = child_directory.lock().unwrap().clone();

    let wallpapers = file_utils::reload_directory(&directory)?;
    log::info!("Reloaded directory");

    // If the wallpaper's directory is empty, we should return an error and leave the index unchanged
    if wallpapers.len() == 0 {
        return Err(Box::new(ServerError("Wallpaper directory is empty!")));
    }

    // Increment index until we're on a new wallpaper. This should only ever be a
    // problem when multiple files have the same name or the directory grows in size
    while wallpapers[*index % wallpapers.len()] == current_wallpaper {
        *index += 1;
    }

    *index = *index % wallpapers.len();

    // Queue the next wallpaper
    let mut next_wallpaper = child_wallpaper.lock().unwrap();
    *next_wallpaper = wallpapers[*index].clone();

    log::info!("Queued wallpaper: {}", next_wallpaper);

    // Change wallpaper
    if current_wallpaper.len() > 0 {
        log::info!("Setting wallpaper: {}", &current_wallpaper);
        file_utils::hyprpaper_update(&current_wallpaper)?;
    }
    Ok(())
}
