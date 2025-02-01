use std::{
    error::Error,
    fmt::Display,
    io::{self, Write},
    net::TcpStream,
    path::PathBuf,
};

use crate::packet_utils::build_response;

#[derive(Debug)]
pub struct HyprpaperError(pub String);

impl Display for HyprpaperError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl Error for HyprpaperError {}

pub fn hyprpaper_update(path: &str) -> Result<(), Box<dyn Error>> {
    let preload = format!("hyprctl hyprpaper preload {}", path);

    let stdout = exec_command(&preload)?;
    if stdout != "ok\n" {
        return Err(Box::from(HyprpaperError(stdout)));
    }

    let load = format!("hyprctl hyprpaper wallpaper \', {}\'", path);
    let stdout = exec_command(&load)?;
    if stdout != "ok\n" {
        return Err(Box::from(HyprpaperError(stdout)));
    }

    let unload_unused = format!("hyprctl hyprpaper unload unused");
    let stdout = exec_command(&unload_unused)?;
    if stdout != "ok\n" {
        return Err(Box::new(HyprpaperError(stdout)));
    }
    Ok(())
}

pub fn exec_command(command: &str) -> io::Result<String> {
    let output = std::process::Command::new("bash")
        .arg("-c")
        .arg(command)
        .output()?;

    Ok(output.stdout.iter().map(|&c| char::from(c)).collect())
}

#[test]
fn test_hyprpaper() {
    let result = hyprpaper_update("~/Pictures/Backgrounds/ssg-vegeta.jpg").unwrap();
}

pub fn reload_directory(path: &str) -> io::Result<Vec<String>> {
    let path = PathBuf::from(path).canonicalize()?;

    Ok(std::fs::read_dir(&path)?
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            e.file_type()
                .ok()?
                .is_file()
                .then(|| e.path().to_str().and_then(|s| Some(s.to_string())))?
        })
        .collect())
}

#[test]
fn test_reload_directory() {
    let dir = reload_directory("/home/piyushk/Pictures/Backgrounds").unwrap();
    dbg!(dir.len(), dir);
}
