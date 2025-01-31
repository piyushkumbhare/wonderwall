use std::{
    collections::HashMap,
    error::Error,
    fmt::Display,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, Condvar, Mutex},
};

use crate::utils::{self, PacketError, ServerError, WallpaperPacket};

pub struct WallpaperServer {
    directory: String,
}

impl WallpaperServer {
    /// Initializes a `WallpaperServer` instance with a backgrounds directory. The server can then be started with `.start()`
    pub fn new(directory: String) -> Self {
        WallpaperServer { directory }
    }

    /// Starts the Wallpaper TCP server.
    ///
    /// If the server is terminated with a `Stop` via TCP, this function will return `Ok(())`.
    ///
    /// If the server encounters a critical error, it will quit and propagate it by returning an `Err(_)`.
    pub fn start<'a>(&mut self) -> Result<(), Box<dyn Error>> {
        log::info!("Using pictures from {}", self.directory);

        // Get the first entry of the start directory to use
        let first_wallpaper = match std::fs::read_dir(&self.directory)?.find_map(|entry| entry.ok())
        {
            Some(e) => e.path().to_string_lossy().to_string(),
            None => {
                return Err(Box::new(ServerError(
                    "Directory was empty or no valid pictures could be loaded!",
                )))
            }
        };

        // TODO: Call hyprctl hyprpaper and load in the first wallpaper
        utils::hyprpaper_update(&first_wallpaper)?;

        // Initialize TCP Listener
        let listener = TcpListener::bind("127.0.0.1:6969")?;

        let trigger = Arc::new((Mutex::new(false), Condvar::new()));
        let trigger_clone = trigger.clone();

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
        let Ok(buffer) = utils::extract_string_buffered(&mut reader) else {
            log::error!("Error while attempting to read from TCP stream");
            utils::send_empty_response(&stream);
            return;
        };
        log::info!("Request received");

        let packet = match utils::decode_packet(buffer) {
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
                utils::send_empty_response(&stream);
                return;
            }
        };

        // If the WallpaperControl header is not present, discard packet
        if !packet.headers.contains_key("WallpaperControl") {
            log::warn!("Necessary packet header not found.");
            utils::send_empty_response(&stream);
            return;
        }

        let response = match self.process_packet(packet) {
            Ok(body) => utils::build_packet(true, Some(body)),
            Err(e) => utils::build_packet(false, Some(format!("{e}"))),
        };

        log::info!("Replying with packet:\n{response}");
        match stream.write_all(response.as_bytes()) {
            Ok(_) => {}
            Err(e) => {
                log::error!("Error when attempting to write to TCP stream: {e}");
                return;
            }
        }
    }

    /// Processes a decoded packet's request.
    fn process_packet<'a>(&mut self, packet: WallpaperPacket) -> Result<String, Box<dyn Error>> {
        let command = packet.headers["WallpaperControl"].clone();
        let response: String = match command.as_str() {
            "Update" => {
                let path = packet.body;
                utils::hyprpaper_update(&path)?;

                format!("Successfully updated wallpaper to {path}")
            }
            "Cycle" => {
                log::info!("Received Cycle request");
                todo!()
            }
            "Stop" => {
                log::info!("Received Stop request, closing server...");
                std::process::exit(0);
            }
            "GetDir" => {
                log::info!("Received GetDir request");

                format!(
                    "The wallpaper engine is cycling through pictures in {}",
                    self.directory
                )
            }
            "SetDir" => {
                log::info!("Received SetDir request");

                let new_dir = packet.body;
                self.directory = new_dir;
                // TODO: Add a check to see if the directory is valid. If not, then echo a response with a descriptive body

                format!(
                    "The wallpaper engine will now cycle through pictures in {}",
                    self.directory
                )
            }
            _ => {
                return Err(Box::new(PacketError(
                    "Received unknown request, discarding packet...",
                )))
            }
        };

        Ok(response.to_string())
    }
}
