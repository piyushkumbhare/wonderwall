use std::{
    error::Error,
    fmt::Display,
    io::{BufReader, Write},
    os::unix::net::{UnixListener, UnixStream},
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
};

use crate::{
    constants::*,
    utils::{socket_utils::Packet, *},
};

/// Options the user can pass in to WallpaperServer::new()
#[derive(Debug)]
pub struct WallpaperOptions {
    pub directory: String,
    pub duration: u64,
    pub recursive: bool,
    pub random: bool,
}

pub struct WallpaperData {
    pub directory: String,
    pub current_wallpaper: String,
    pub next_wallpaper: String,
    pub recursive: bool,
    pub random: bool,
    pub index: usize,
}

pub struct WallpaperServer {
    pub duration: u64,
    pub main_trigger: Arc<(Mutex<bool>, Condvar)>,
    pub data: Arc<Mutex<WallpaperData>>,
}

impl Drop for WallpaperServer {
    fn drop(&mut self) {
        log::warn!("Removing file {}", FILE_SOCKET);
        std::fs::remove_file(FILE_SOCKET).expect("Failed to remove socket file.");
    }
}

impl WallpaperServer {
    /// Initializes a `WallpaperServer` instance with a backgrounds directory. The server can then be started with `.start()`
    pub fn new(
        WallpaperOptions {
            directory,
            duration,
            recursive,
            random,
        }: WallpaperOptions,
    ) -> Result<Self, Box<dyn Error>> {
        // If the path exists, try pinging the server
        if Path::new(&FILE_SOCKET).exists() {
            if socket_utils::send_request("PING", "", FILE_SOCKET)
                .is_ok_and(|response| response.trim() == "pong")
            {
                // If the server responds, it means its running, so we back off
                log::error!("Server is alraedy running on socket!");
                return Err(Box::new(ServerError::SocketError(
                    "Server is already running on socket!",
                )));
            } else {
                // If the server did not respond, it was most likely improperly terminated, so we take over
                log::warn!("Socket file was detected, but server did not respond to ping. Deleting socket and starting server...");
                std::fs::remove_file(FILE_SOCKET).unwrap();
            }
        }

        // Read the directory
        let wallpapers = file_utils::get_directory_files(&PathBuf::from(&directory), recursive)?;

        let first_index = match random {
            true => rand::random_range(..wallpapers.len()),
            false => 0,
        };

        let second_index = match random {
            true => {
                let mut second_index = rand::random_range(..wallpapers.len());
                while second_index == first_index {
                    second_index = rand::random_range(..wallpapers.len())
                }
                second_index
            }
            false => 1,
        };

        let first_wallpaper = wallpapers
            .get(first_index)
            .unwrap_or(&String::new())
            .clone();

        let second_wallpaper = wallpapers
            .get(second_index)
            .unwrap_or(&String::new())
            .clone();

        Ok(WallpaperServer {
            main_trigger: Arc::new((Mutex::new(false), Condvar::new())),
            duration,
            data: Arc::new(Mutex::new(WallpaperData {
                directory,
                current_wallpaper: first_wallpaper,
                next_wallpaper: second_wallpaper,
                recursive,
                random,
                index: 0,
            })),
        })
    }

    /// Starts the Wallpaper socket server.
    ///
    /// If the server is terminated with a `Stop` via Unix Socket request, this function will return `Ok(())`.
    ///
    /// If the server encounters a critical error, it will quit and propagate it by returning an `Err(_)`.
    pub fn run(&mut self) -> Result<(), Box<dyn Error>> {
        // Set up Atomic Mutexes for the child thread to use
        let child_trigger = self.main_trigger.clone();
        let child_data = self.data.clone();
        let duration = self.duration;

        // Spawn the child thread. This thread will be responsible for cycling the wallpaper every DURATION seconds
        std::thread::spawn(move || -> ! {
            loop {
                match cycle_wallpapers(duration, &child_trigger, &child_data) {
                    Ok(_) => {}
                    Err(e) => {
                        log::warn!("Ran into error: {e}");
                        match e {
                            ServerError::FileError(msg) => {
                                if msg != "Empty directory" {
                                    log::error!("FATAL ERROR. Terminating...");
                                    std::fs::remove_file(FILE_SOCKET)
                                        .expect("Failed to remove socket file.");
                                    std::process::exit(1);
                                }
                            }
                            ServerError::HyprpaperError => {
                                log::error!("FATAL ERROR. Terminating...");
                                std::fs::remove_file(FILE_SOCKET)
                                    .expect("Failed to remove socket file.");
                                std::process::exit(1);
                            }
                            _ => {}
                        }
                    }
                }
            }
        });

        let listener = UnixListener::bind(FILE_SOCKET)?;

        log::info!("Starting server at {}", FILE_SOCKET);

        // Start listening for requests on the File socket!
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    if let Err(error_type) = self.handle_stream(stream) {
                        match error_type {
                            ServerError::Kill => {
                                log::warn!("Stopping server...");
                                // Break out of the socket listener loop so we can exit gracefully through `main()`
                                break;
                            }
                            e => {
                                log::error!("{e}");
                            }
                        }
                    };
                }
                Err(e) => {
                    log::error!("Ran into an error when handling request: {e}");
                    continue;
                }
            };
        }
        Ok(())
    }

    /// Reads the raw request from socket bytestream, decodes the packet, and submits the request to be processed.
    fn handle_stream(&mut self, mut stream: UnixStream) -> Result<(), ServerError> {
        // Read bytes into the buffer using a reader
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let Ok(buffer) = socket_utils::extract_bytes_buffered(&mut reader) else {
            // Reading bytes is an internal error
            let response = Packet::new().method("300").body("Internal server error");
            stream
                .write_all(&response.as_bytes())
                .map_err(|_| ServerError::SocketError(SOCKET_WRITE_ERROR))?;

            return Err(ServerError::RequestError(
                "Error while attempting to read from File Socket stream",
            ));
        };

        log::info!(
            "Request received\n`{}`",
            String::from_utf8(buffer.clone()).unwrap()
        );

        let Ok(request) = Packet::from_bytes(buffer) else {
            // Bad packet format is a user error
            let response = Packet::new().method("400").body("Request has bad format");
            stream
                .write_all(&response.as_bytes())
                .map_err(|_| ServerError::SocketError(SOCKET_WRITE_ERROR))?;

            return Err(ServerError::RequestError("Packet has bad format"));
        };

        let command = match request.headers.get("WallpaperControl") {
            Some(command) => command,
            None => {
                // Bad packet format is a user error
                let response = Packet::new().method("400").body("Missing required headers");
                stream
                    .write_all(&response.as_bytes())
                    .map_err(|_| ServerError::SocketError(SOCKET_WRITE_ERROR))?;

                return Err(ServerError::RequestError(
                    "Packet is missing required headers",
                ));
            }
        };

        // Handle Wallpaper command
        match command.to_uppercase().as_str() {
            "GETWP" => self.get_wp(&mut stream)?,
            "SETWP" => self.set_wp(&mut stream, request.body)?,
            "NEXT" => self.next(&mut stream)?,
            "GETDIR" => self.get_dir(&mut stream)?,
            "SETDIR" => self.set_dir(&mut stream, request.body)?,
            "KILL" => self.kill(&mut stream)?,
            "PING" => self.ping(&mut stream)?,
            invalid_request => {
                log::warn!("Received invalid request: {invalid_request}");

                // Invalid request is a user error
                let response = Packet::new().method("400").body("Invalid request!");
                stream
                    .write_all(&response.as_bytes())
                    .map_err(|_| ServerError::SocketError(SOCKET_WRITE_ERROR))?;
            }
        }
        Ok(())
    }
}

/// Ran by the child thread to periodically cycle wallpapers
///
/// Internally increments `index`.
fn cycle_wallpapers<'a>(
    duration: u64,
    child_trigger: &'a Arc<(Mutex<bool>, Condvar)>,
    child_data: &'a Arc<Mutex<WallpaperData>>,
) -> Result<(), ServerError<'a>> {
    let mut data = child_data.lock().unwrap();

    let wallpapers =
        file_utils::get_directory_files(&PathBuf::from(&data.directory), data.recursive).map_err(
            |e| {
                log::error!("{e}");
                ServerError::FileError("Error in reading directory")
            },
        )?;

    log::info!("Reloaded directory");

    // If the wallpaper's directory is empty, we should return an error and leave the index unchanged
    if wallpapers.is_empty() {
        return Err(ServerError::FileError("Empty directory"));
    }

    // Change index until we're on a new wallpaper. This should only ever be a
    // problem when multiple files have the same name or the directory grows in size
    while wallpapers[data.index % wallpapers.len()] == data.next_wallpaper {
        match data.random {
            true => data.index = rand::random_range(..wallpapers.len()),
            false => data.index += 1,
        }
    }

    data.index %= wallpapers.len();

    // Queue the next wallpaper
    data.current_wallpaper = data.next_wallpaper.clone();
    data.next_wallpaper = wallpapers[data.index].clone();

    log::info!("Queued wallpaper: {}", data.current_wallpaper);

    // Change wallpaper
    log::info!("Setting wallpaper: {}", &data.current_wallpaper);
    file_utils::hyprpaper_update(&data.current_wallpaper)
        .map_err(|_| ServerError::HyprpaperError)?;

    drop(data);
    // Wait for trigger or timeout
    let (lock, cvar) = &**child_trigger;
    let triggered = lock.lock().unwrap();
    let _ = cvar.wait_timeout(triggered, std::time::Duration::from_secs(duration));

    Ok(())
}

// Server Error implementations

#[derive(Debug)]
pub enum ServerError<'a> {
    Kill,
    HyprpaperError,
    RequestError(&'a str),
    SocketError(&'a str),
    FileError(&'a str),
}

impl Display for ServerError<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServerError::Kill => f.write_str("Killed"),
            ServerError::HyprpaperError => f.write_str("Hyprpaper crashed!"),
            ServerError::RequestError(msg) => f.write_str(msg),
            ServerError::SocketError(msg) => f.write_str(msg),
            ServerError::FileError(msg) => f.write_str(msg),
        }
    }
}

impl Error for ServerError<'_> {}
