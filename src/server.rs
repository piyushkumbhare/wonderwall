use std::{
    error::Error,
    io::{BufReader, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, Condvar, Mutex},
};

use crate::{
    file_utils,
    network_utils::{self, Packet, ServerError},
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

        let first_wallpaper = wallpapers.first().unwrap_or(&String::new()).clone();

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
            match stream {
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
        let Ok(buffer) = network_utils::extract_bytes_buffered(&mut reader) else {
            log::error!("Error while attempting to read from TCP stream");

            let response = Packet::new().method("300").body("Internal server error");
            stream.write_all(&response.as_bytes()).unwrap_or_else(|_| {
                log::error!("Failed to write to TCP Stream!");
            });
            return;
        };

        log::info!("Request received");

        log::info!("\n`{:?}`", buffer);
        log::info!("\n`{}`", String::from_utf8(buffer.clone()).unwrap());

        let Ok(request) = Packet::from_bytes(buffer) else {
            log::error!("Packet has bad format");

            let response = Packet::new().method("400").body("Request has bad format");
            stream.write_all(&response.as_bytes()).unwrap_or_else(|_| {
                log::error!("Failed to write to TCP Stream!");
            });
            return;
        };

        let command = match request.headers.get("WallpaperControl") {
            Some(command) => command,
            None => {
                log::error!("Packet is missing required headers");

                let response = Packet::new().method("400").body("Missing required headers");
                stream.write_all(&response.as_bytes()).unwrap_or_else(|_| {
                    log::error!("Failed to write to TCP Stream!");
                });
                return;
            }
        };

        // Handle Wallpaper command
        match command.to_uppercase().as_str() {
            "UPDATE" => {
                log::info!("Received request: UPDATE");
                *self.wallpaper.lock().unwrap() = request.body.clone();
                let (lock, cvar) = &*self.main_trigger;

                let mut trigger = lock.lock().unwrap();
                *trigger = true;
                cvar.notify_one();

                let response = Packet::new()
                    .method("200")
                    .body(format!("Updated wallpaper to {}", request.body).as_str());

                stream.write_all(&response.as_bytes()).unwrap_or_else(|_| {
                    log::error!("Failed to write to TCP Stream!");
                });
            }
            "NEXT" => {
                log::info!("Received request: NEXT");
                let (lock, cvar) = &*self.main_trigger;
                let next_wallpaper = self.wallpaper.lock().unwrap().clone();

                let mut trigger = lock.lock().unwrap();
                *trigger = true;
                cvar.notify_one();

                let response = Packet::new()
                    .method("200")
                    .body(format!("Cycled wallpaper to {}", next_wallpaper).as_str());
                stream.write_all(&response.as_bytes()).unwrap_or_else(|_| {
                    log::error!("Failed to write to TCP Stream!");
                });
            }
            "GETDIR" => {
                log::info!("Received request: GETDIR");

                let cur_dir = self.directory.lock().unwrap().clone();
                let response = Packet::new().method("200").body(&cur_dir);
                stream.write_all(&response.as_bytes()).unwrap_or_else(|_| {
                    log::error!("Failed to write to TCP Stream!");
                });
            }
            "SETDIR" => {
                log::info!("Received request: SETDIR");

                // Attempt to set the new directory
                match file_utils::reload_directory(request.body.trim()) {
                    Ok(_) => {
                        // If successful, set the directory and respond with 200
                        *self.directory.lock().unwrap() = request.body.clone();

                        let response = Packet::new().method("200").body(
                            format!("Wonderwall will now cycle through {}", request.body).as_str(),
                        );
                        stream.write_all(&response.as_bytes()).unwrap_or_else(|_| {
                            log::error!("Failed to write to TCP Stream!");
                        })
                    }
                    Err(e) => {
                        // If failed, respond with 400
                        let response = Packet::new().method("400").body(
                            format!("There was an error setting the directory: {e}").as_str(),
                        );
                        stream.write_all(&response.as_bytes()).unwrap_or_else(|_| {
                            log::error!("Failed to write to TCP Stream!");
                        })
                    }
                };
            }
            "KILL" => {
                log::info!("Received request: KILL");

                let response = Packet::new().method("200").body("Stopping server...");
                stream.write_all(&response.as_bytes()).unwrap_or_else(|_| {
                    log::error!("Failed to write to TCP Stream!");
                });

                std::process::exit(0);
            }
            r => {
                log::warn!("Received invalid request: {r}");

                let response = Packet::new().method("400").body("Invalid request!");
                stream.write_all(&response.as_bytes()).unwrap_or_else(|_| {
                    log::error!("Failed to write to TCP Stream!");
                })
            }
        }
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
    let triggered = lock.lock().unwrap();
    let _ = cvar.wait_timeout(triggered, std::time::Duration::from_secs(duration));

    // Read next wallpaper
    let current_wallpaper = child_wallpaper.lock().unwrap().clone();

    // Reload directory
    let directory = child_directory.lock().unwrap().clone();

    let wallpapers = file_utils::reload_directory(&directory)?;
    log::info!("Reloaded directory");

    // If the wallpaper's directory is empty, we should return an error and leave the index unchanged
    if wallpapers.is_empty() {
        return Err(Box::new(ServerError("Wallpaper directory is empty!")));
    }

    // Increment index until we're on a new wallpaper. This should only ever be a
    // problem when multiple files have the same name or the directory grows in size
    while wallpapers[*index % wallpapers.len()] == current_wallpaper {
        *index += 1;
    }

    *index %= wallpapers.len();

    // Queue the next wallpaper
    let mut next_wallpaper = child_wallpaper.lock().unwrap();
    *next_wallpaper = wallpapers[*index].clone();

    log::info!("Queued wallpaper: {}", next_wallpaper);

    // Change wallpaper
    if !current_wallpaper.is_empty() {
        log::info!("Setting wallpaper: {}", &current_wallpaper);
        file_utils::hyprpaper_update(&current_wallpaper)?;
    }
    Ok(())
}
