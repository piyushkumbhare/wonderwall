use std::{io::Write, os::unix::net::UnixStream};

use crate::{
    constants::*,
    utils::{socket_utils::Packet, *},
};

use super::server::*;

impl WallpaperServer {
    pub fn update(&mut self, stream: &mut UnixStream, value: String) -> Result<(), ServerError> {
        log::info!("Received request: UPDATE");
        *self.wallpaper.lock().unwrap() = value.clone();
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

    pub fn next(&mut self, stream: &mut UnixStream) -> Result<(), ServerError> {
        log::info!("Received request: NEXT");
        let (lock, cvar) = &*self.main_trigger;
        let next_wallpaper = self.wallpaper.lock().unwrap().clone();

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

        let cur_dir = self.directory.lock().unwrap().clone();
        let response = Packet::new().method("200").body(&cur_dir);
        stream
            .write_all(&response.as_bytes())
            .map_err(|_| ServerError::SocketError(SOCKET_WRITE_ERROR))?;
        Ok(())
    }

    pub fn set_dir(&mut self, stream: &mut UnixStream, value: String) -> Result<(), ServerError> {
        log::info!("Received request: SETDIR");

        // Attempt to set the new directory
        match file_utils::get_directory_files(value.trim()) {
            Ok(contents) => {
                // If successful, set the directory and respond with 200
                *self.directory.lock().unwrap() = value.clone();

                let response = Packet::new()
                    .method("200")
                    .body(format!("Wonderwall will now cycle through {}", value).as_str());

                stream
                    .write_all(&response.as_bytes())
                    .map_err(|_| ServerError::SocketError(SOCKET_WRITE_ERROR))?;

                if let Some(new_first_wallpaper) = contents.first() {
                    *self.wallpaper.lock().unwrap() = new_first_wallpaper.clone();
                    let (lock, cvar) = &*self.main_trigger;

                    let mut trigger = lock.lock().unwrap();
                    *trigger = true;
                    cvar.notify_one();
                    log::info!("Updated wallpaper due to SETDIR request");
                }
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
