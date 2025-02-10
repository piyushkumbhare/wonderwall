use clap::Parser;

mod args;
mod constants;
mod utils;
mod wpserver;

use args::*;
use wpserver::server::WallpaperServer;

use utils::socket_utils;

use constants::*;
fn main() {
    let args = Args::parse();

    // Enable logging if verbose
    if args.verbose {
        env_logger::Builder::new()
            .filter_level(log::LevelFilter::Debug)
            .init();
    }

    if args.directory.is_some() {
        let directory = args.directory.clone().unwrap();
        let mut server = WallpaperServer::new(directory, args.duration, FILE_SOCKET).unwrap();
        match server.start() {
            Ok(_) => {
                log::info!("Server started!");
                println!("Server started!");
            }
            Err(e) => {
                log::error!("Ran into error: {e}");
                eprintln!("Ran into error: {e}");
            }
        }
    } else {
        let request_result = match args.action() {
            Action::Update(wallpaper) => {
                socket_utils::send_request("UPDATE", &wallpaper, FILE_SOCKET)
            }
            Action::Next => socket_utils::send_request("NEXT", "", FILE_SOCKET),
            Action::GetDir => socket_utils::send_request("GETDIR", "", FILE_SOCKET),
            Action::SetDir(directory) => {
                socket_utils::send_request("SETDIR", &directory, FILE_SOCKET)
            }
            Action::Kill => socket_utils::send_request("KILL", "", FILE_SOCKET),
        };

        match request_result {
            Ok(response) => {
                log::info!("Received response: {response}");
                println!("{response}");
            }
            Err(e) => {
                log::error!("Ran into error: {e}");
                eprintln!("{e}");
            }
        }
    }
}
