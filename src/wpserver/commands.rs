use std::{io::Write, os::unix::net::UnixStream, path::PathBuf};

use crate::{
    constants::*,
    utils::{socket_utils::Packet, *},
};

use super::server::*;

impl WallpaperServer {
    pub fn set_wp(&mut self, stream: &mut UnixStream, value: String) -> Result<(), ServerError> {
        log::info!("Received request: SETWP");
        let mut data = self.data.lock().unwrap();

        data.next_wallpaper = value.clone();

        // Trigger wallpaper switch event
        let (lock, cvar) = &*self.main_trigger;

        let mut trigger = lock.lock().unwrap();
        *trigger = true;
        cvar.notify_one();

        let response = Packet::new()
            .method("200")
            .body(format!("Updated wallpaper to {}", value).as_str());

        stream
            .write_all(&response.as_bytes())
            .map_err(|_| ServerError::SocketError(SOCKET_WRITE_ERROR))?;
        Ok(())
    }

    pub fn get_wp(&mut self, stream: &mut UnixStream) -> Result<(), ServerError> {
        log::info!("Received request: GETWP");
        let data = self.data.lock().unwrap();

        let cur_wp = data.current_wallpaper.clone();
        let response = Packet::new().method("200").body(&cur_wp);
        stream
            .write_all(&response.as_bytes())
            .map_err(|_| ServerError::SocketError(SOCKET_WRITE_ERROR))?;
        Ok(())
    }

    pub fn next(&mut self, stream: &mut UnixStream) -> Result<(), ServerError> {
        log::info!("Received request: NEXT");
        let data = self.data.lock().unwrap();

        let (lock, cvar) = &*self.main_trigger;
        let next_wallpaper = data.current_wallpaper.clone();

        let mut trigger = lock.lock().unwrap();
        *trigger = true;
        cvar.notify_one();

        let response = Packet::new()
            .method("200")
            .body(format!("Cycled wallpaper to {}", next_wallpaper).as_str());
        stream
            .write_all(&response.as_bytes())
            .map_err(|_| ServerError::SocketError(SOCKET_WRITE_ERROR))?;
        Ok(())
    }

    pub fn get_dir(&mut self, stream: &mut UnixStream) -> Result<(), ServerError> {
        log::info!("Received request: GETDIR");
        let data = self.data.lock().unwrap();

        let cur_dir = data.directory.clone();
        let response = Packet::new().method("200").body(&cur_dir);
        stream
            .write_all(&response.as_bytes())
            .map_err(|_| ServerError::SocketError(SOCKET_WRITE_ERROR))?;
        Ok(())
    }

    pub fn set_dir(&mut self, stream: &mut UnixStream, value: String) -> Result<(), ServerError> {
        log::info!("Received request: SETDIR");
        let mut data = self.data.lock().unwrap();

        let mut fields = value.splitn(3, '\n');

        let Some(recursive) = fields.next() else {
            return Err(ServerError::RequestError("Invalid request format"));
        };

        let Some(random) = fields.next() else {
            return Err(ServerError::RequestError("Invalid request format"));
        };

        let Some(path) = fields.next() else {
            return Err(ServerError::RequestError("Invalid request format"));
        };

        data.recursive = recursive.is_empty();
        data.random = random.is_empty();

        // Attempt to set the new directory
        match file_utils::get_directory_files(&PathBuf::from(path), recursive.is_empty()) {
            Ok(contents) => {
                // If successful, set the directory, load the first wallpaper, and respond with 200
                data.directory = path.to_string().clone();

                if let Some(new_first_wallpaper) = contents.first() {
                    data.current_wallpaper = new_first_wallpaper.clone();
                    let (lock, cvar) = &*self.main_trigger;

                    let mut trigger = lock.lock().unwrap();
                    *trigger = true;
                    cvar.notify_one();
                    log::info!("Updated wallpaper due to SETDIR request");
                }

                let response = Packet::new()
                    .method("200")
                    .body(format!("Wonderwall will now cycle through {}", path).as_str());

                stream
                    .write_all(&response.as_bytes())
                    .map_err(|_| ServerError::SocketError(SOCKET_WRITE_ERROR))?;
            }
            Err(e) => {
                // If failed, respond with 400
                let response = Packet::new()
                    .method("400")
                    .body(format!("There was an error setting the directory: {e}").as_str());
                stream
                    .write_all(&response.as_bytes())
                    .map_err(|_| ServerError::SocketError(SOCKET_WRITE_ERROR))?;
            }
        };
        Ok(())
    }

    pub fn kill(&mut self, stream: &mut UnixStream) -> Result<(), ServerError> {
        log::info!("Received request: KILL");

        let response = Packet::new().method("200").body("Stopping server...");

        stream
            .write_all(&response.as_bytes())
            .map_err(|_| ServerError::SocketError(SOCKET_WRITE_ERROR))?;

        Err(ServerError::Kill)
    }

    #[allow(unused)]
    pub fn ping(&mut self, stream: &mut UnixStream) -> Result<(), ServerError> {
        log::info!("Received request: PING");

        let response = Packet::new().method("200").body("pong");
        stream
            .write_all(&response.as_bytes())
            .map_err(|_| ServerError::SocketError(SOCKET_WRITE_ERROR))
    }
}
