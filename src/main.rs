use std::error::Error;

use clap::Parser;

// Can Rust PLEASE add a way to bundle `mod` statements
mod args;
mod constants;
mod utils;
mod wpserver;

use args::*;
use constants::*;
use utils::socket_utils;
use wpserver::server::WallpaperServer;

// TODO: See if there's a better way to return out of main... I don't like unnecessarily using Box<dyn Error>.
// Also for some reason, anyhow::Result<()> won't work with nix::unistd::daemon()'s Error variant
fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    // Enable logging if verbose
    if args.verbose {
        env_logger::Builder::new()
            .filter_level(log::LevelFilter::Debug)
            .init();
    }

    // Parse subcommand
    use Opt::*;
    match args.command {
        Start {
            directory,
            duration,
            run_here,
        } => {
            let mut server = match WallpaperServer::new(directory, duration, FILE_SOCKET) {
                Ok(s) => s,
                Err(e) => {
                    log::error!("Ran into error while creating server: {e}");
                    eprintln!("Ran into error while creating server {e}");
                    std::process::exit(1);
                }
            };

            // If not running in the current terminal, attempt to detatch
            if !run_here {
                log::warn!("Attempting to detatch from parent terminal... (You won't get a message if it was successful btw lol)");
                if let Err(e) = nix::unistd::daemon(false, false) {
                    log::error!("Error while trying to daemonize: {e}");
                    return Err(Box::new(e));
                };
                log::info!("Server is now a fully realized daemon. Yay! >:)");
            } else {
                log::info!("Running server in current terminal!");
            }

            // Attempt to run the server. This block will only ever return if an error occurs or if the server is manually shut down
            match server.run() {
                Ok(_) => log::info!("Server stopped successfully!"),
                Err(e) => {
                    log::error!("Ran into error while running server: {e}");
                    eprintln!("Ran into error while running server: {e}");
                    return Err(e);
                }
            }
        }
        command => {
            // Parse the command and send the appropriate request
            let request_result = match command {
                Update { path } => socket_utils::send_request("UPDATE", &path, FILE_SOCKET),
                Next => socket_utils::send_request("NEXT", "", FILE_SOCKET),
                GetDir => socket_utils::send_request("GETDIR", "", FILE_SOCKET),
                SetDir { directory } => {
                    socket_utils::send_request("SETDIR", &directory, FILE_SOCKET)
                }
                Ping => socket_utils::send_request("PING", "", FILE_SOCKET),
                Kill => socket_utils::send_request("KILL", "", FILE_SOCKET),
                _ => unreachable!(), // Won't be reached since we already matched all possible subcommands
            };

            // Get the response/error and print it to the screen
            match request_result {
                Ok(response) => println!("{response}"),
                Err(e) => eprintln!("Ran into error while sending request: {e}"),
            }
        }
    }
    Ok(())
}
