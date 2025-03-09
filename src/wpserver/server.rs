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

pub struct WallpaperServer {
    pub directory: Arc<Mutex<String>>, // TODO: Add a flatten feature that recursively unfolds all subdirectories
    pub wallpaper: Arc<Mutex<String>>,
    pub main_trigger: Arc<(Mutex<bool>, Condvar)>,
    pub duration: u64,
    pub socket: String,
    pub recursive: Arc<Mutex<bool>>,
}

impl Drop for WallpaperServer {
    fn drop(&mut self) {
        log::warn!("Removing file {}", &self.socket);
        std::fs::remove_file(&self.socket).expect("Failed to remove socket file.");
    }
}

impl WallpaperServer {
    /// Initializes a `WallpaperServer` instance with a backgrounds directory. The server can then be started with `.start()`
    pub fn new(
        directory: String,
        duration: u64,
        socket: &str,
        recursive: bool,
    ) -> Result<Self, Box<dyn Error>> {
        // FIXME: What the fuck is this... please load the first wallpaper to the screen and queue the SECOND one
        let wallpapers = file_utils::get_directory_files(&PathBuf::from(&directory), recursive)?;
        let first_wallpaper = wallpapers.first().unwrap_or(&String::new()).clone();

        // If the path exists, try pinging the server
        if Path::new(&socket).exists() {
            if socket_utils::send_request("PING", "", socket)
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
                std::fs::remove_file(socket).unwrap();
            }
        }

        Ok(WallpaperServer {
            directory: Arc::new(Mutex::new(directory)),
            wallpaper: Arc::new(Mutex::new(first_wallpaper)),
            main_trigger: Arc::new((Mutex::new(false), Condvar::new())),
            socket: socket.to_string(),
            duration,
            recursive: Arc::new(Mutex::new(recursive)),
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
        let child_wallpaper = self.wallpaper.clone();
        let child_directory = self.directory.clone();
        let mut index = 0;
        let duration = self.duration;
        let child_recursive = self.recursive.clone();

        // TODO: Look for better ways to do this... I don't like how an entirely new function is needed just because `&self.<anything>``
        // causes borrow-checker errors due to `self` being moved

        // Spawn the child thread. This thread will be responsible for cycling the wallpaper every DURATION seconds
        std::thread::spawn(move || -> ! {
            loop {
                match cycle_wallpapers(
                    &child_trigger,
                    &child_wallpaper,
                    &child_directory,
                    &mut index,
                    duration,
                    &child_recursive,
                ) {
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

        let listener = UnixListener::bind(&self.socket)?;

        log::info!("Starting server at {}", &self.socket);
        log::info!("Wallpaper will cycle every {} seconds", &self.duration);

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
            "UPDATE" => self.update(&mut stream, request.body)?,
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
    child_trigger: &'a Arc<(Mutex<bool>, Condvar)>,
    child_wallpaper: &'a Arc<Mutex<String>>,
    child_directory: &'a Arc<Mutex<String>>,
    index: &'a mut usize,
    duration: u64,
    recursive: &'a Arc<Mutex<bool>>,
) -> Result<(), ServerError<'a>> {
    let (lock, cvar) = &**child_trigger;

    // Wait for trigger or timeout
    let triggered = lock.lock().unwrap();
    let _ = cvar.wait_timeout(triggered, std::time::Duration::from_secs(duration));

    // Read next wallpaper
    let current_wallpaper = child_wallpaper.lock().unwrap().clone();

    // Reload directory
    let directory = child_directory.lock().unwrap().clone();

    let wallpapers = file_utils::get_directory_files(
        &PathBuf::from(&directory),
        recursive.lock().unwrap().clone(),
    )
    .map_err(|e| {
        log::error!("{e}");
        ServerError::FileError("Error in reading directory")
    })?;

    log::info!("Reloaded directory");

    // If the wallpaper's directory is empty, we should return an error and leave the index unchanged
    if wallpapers.is_empty() {
        return Err(ServerError::FileError("Empty directory"));
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
        file_utils::hyprpaper_update(&current_wallpaper)
            .map_err(|_| ServerError::HyprpaperError)?;
    }
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
