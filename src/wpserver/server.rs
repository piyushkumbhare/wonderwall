use std::{
    error::Error,
    fmt::Display,
    io::{BufReader, Write},
    os::unix::net::{UnixListener, UnixStream},
    path::Path,
    sync::{Arc, Condvar, Mutex},
};

use crate::{
    constants,
    utils::{socket_utils::Packet, *},
};

pub struct WallpaperServer {
    pub directory: Arc<Mutex<String>>,
    pub wallpaper: Arc<Mutex<String>>,
    pub main_trigger: Arc<(Mutex<bool>, Condvar)>,
    pub duration: u64,
    pub socket: String,
}

impl Drop for WallpaperServer {
    fn drop(&mut self) {
        log::info!("Removing file {}", &self.socket);
        let _ = std::fs::remove_file(&self.socket);
    }
}

impl WallpaperServer {
    /// Initializes a `WallpaperServer` instance with a backgrounds directory. The server can then be started with `.start()`
    pub fn new(directory: String, duration: u64, socket: &str) -> Result<Self, Box<dyn Error>> {
        let wallpapers = file_utils::reload_directory(&directory)?;

        let first_wallpaper = wallpapers.first().unwrap_or(&String::new()).clone();

        // Initialize File Listener
        if Path::new(&socket).exists() {
            match UnixListener::bind(socket) {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
                    return Err(Box::new(ServerError::RequestError(
                        "Socket seems to be in use",
                    )));
                }
                Err(e) => {
                    log::error!("{e}");
                    std::fs::remove_file(socket)?;
                    return Err(Box::new(ServerError::RequestError(
                        "Removed socket file because unable to connect. Try again.",
                    )));
                }
            }
        }

        Ok(WallpaperServer {
            directory: Arc::new(Mutex::new(directory)),
            wallpaper: Arc::new(Mutex::new(first_wallpaper)),
            main_trigger: Arc::new((Mutex::new(false), Condvar::new())),
            socket: socket.to_string(),
            duration,
        })
    }

    /// Starts the Wallpaper File server.
    ///
    /// If the server is terminated with a `Stop` via File socket, this function will return `Ok(())`.
    ///
    /// If the server encounters a critical error, it will quit and propagate it by returning an `Err(_)`.
    pub fn start(&mut self) -> Result<(), Box<dyn Error>> {
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
                Err(e) => {
                    log::error!("Ran into error: {e}");
                    std::process::exit(1);
                }
            }
        });

        log::info!("Starting server at {}", &self.socket);
        log::info!("Wallpaper will cycle every {} seconds", &self.duration);

        let listener = UnixListener::bind(&self.socket)?;

        // Start listening for requests on the File socket!
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    if let Err(error_type) = self.handle_stream(stream) {
                        match error_type {
                            ServerError::Kill => {
                                log::warn!("Stopping server...");
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

    /// Reads the raw request from File bytestream, decodes the packet, and submits the request to be processed.
    fn handle_stream(&mut self, mut stream: UnixStream) -> Result<(), ServerError> {
        // Read bytes into the buffer using a reader
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let Ok(buffer) = socket_utils::extract_bytes_buffered(&mut reader) else {
            let response = Packet::new().method("300").body("Internal server error");
            stream.write_all(&response.as_bytes()).unwrap_or_else(|_| {
                log::error!("Failed to write to File Socket Stream!");
            });
            return Err(ServerError::RequestError(
                "Error while attempting to read from File Socket stream",
            ));
        };

        log::info!(
            "Request received\n`{}`",
            String::from_utf8(buffer.clone()).unwrap()
        );

        let Ok(request) = Packet::from_bytes(buffer) else {
            let response = Packet::new().method("400").body("Request has bad format");
            stream.write_all(&response.as_bytes()).unwrap_or_else(|_| {
                log::error!("Failed to write to File Socket Stream!");
            });
            return Err(ServerError::RequestError("Packet has bad format"));
        };

        let command = match request.headers.get("WallpaperControl") {
            Some(command) => command,
            None => {
                let response = Packet::new().method("400").body("Missing required headers");
                stream
                    .write_all(&response.as_bytes())
                    .map_err(|_| ServerError::SocketError(constants::SOCKET_WRITE_ERROR))?;

                return Err(ServerError::RequestError(
                    "Packet is missing required headers",
                ));
            }
        };

        // Handle Wallpaper command
        match command.to_uppercase().as_str() {
            "UPDATE" => self.update(&mut stream, request.body)?,
            "NEXT" => self.next(&mut stream)?,
            "GETDIR" => self.get_dir(&mut stream)?,
            "SETDIR" => self.set_dir(&mut stream, request.body)?,
            "KILL" => self.kill(&mut stream)?,
            invalid_request => {
                log::warn!("Received invalid request: {invalid_request}");

                let response = Packet::new().method("400").body("Invalid request!");
                stream
                    .write_all(&response.as_bytes())
                    .map_err(|_| ServerError::SocketError(constants::SOCKET_WRITE_ERROR))?;
            }
        }
        Ok(())
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
        return Err(Box::new(ServerError::FileError(
            "Wallpaper directory is empty!",
        )));
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

#[derive(Debug)]
pub enum ServerError<'a> {
    Kill,
    RequestError(&'a str),
    SocketError(&'a str),
    FileError(&'a str),
}

impl Display for ServerError<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServerError::Kill => f.write_str("Killed"),
            ServerError::RequestError(msg) => f.write_fmt(format_args!("{msg}")),
            ServerError::SocketError(msg) => f.write_fmt(format_args!("{msg}")),
            ServerError::FileError(msg) => f.write_fmt(format_args!("{msg}")),
        }
    }
}

impl Error for ServerError<'_> {}
