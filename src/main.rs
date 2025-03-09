use std::error::Error;

use clap::Parser;

// Can Rust PLEASE add a way to bundle `mod` statements
mod args;
mod constants;
mod utils;
mod wpserver;

use args::*;
use constants::*;
use fern::Dispatch;
use utils::socket_utils;
use wpserver::server::WallpaperServer;

// TODO: See if there's a better way to return out of main... I don't like unnecessarily using Box<dyn Error>.
// Also for some reason, anyhow::Result<()> won't work with nix::unistd::daemon()'s Error variant
fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    // Parse subcommand
    use Opt::*;
    match args.command {
        // Starts the server with the specified parameters
        Start {
            directory,
            duration,
            fg: run_here,
            log,
            recursive,
        } => {
            let logger = setup_logger();
            if let Some(log_file) = log {
                logger.chain(fern::log_file(log_file)?)
            } else {
                logger
            }
            .apply()?;

            let mut server = match WallpaperServer::new(directory, duration, FILE_SOCKET, recursive) {
                Ok(s) => s,
                Err(e) => {
                    log::error!("Ran into error while creating server: {e}");
                    eprintln!("Ran into error while creating server {e}");
                    return Err(e);
                }
            };

            // If not running in the current terminal, attempt to detatch
            if !run_here {
                log::warn!("Attempting to detatch from parent terminal...");
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
                SetDir { directory, recursive } => {
                    let recursive = match recursive {
                        true => "true",
                        false => "",
                    };
                    socket_utils::send_request("SETDIR", &format!("{},{}", recursive, &directory), FILE_SOCKET)
                }
                Ping => socket_utils::send_request("PING", "", FILE_SOCKET),
                Kill => socket_utils::send_request("KILL", "", FILE_SOCKET),
                _ => unreachable!(), // Won't be reached since we already matched all possible subcommands
            };

            // Get the response/error and print it to the screen
            match request_result {
                Ok(response) => println!("{response}"),
                Err(e) => {
                    eprintln!("Ran into error while sending request: {e}\nIs the server running?")
                }
            }
        }
    }
    Ok(())
}

/// Sets up the bare bones logger. The caller (`main`) can then choose to chain a log file or not
fn setup_logger() -> Dispatch {
    fern::Dispatch::new()
        .format(|out, message, record| {
            let colors =
                fern::colors::ColoredLevelConfig::default().info(fern::colors::Color::Green);
            out.finish(format_args!(
                "[{} {} {}] {}",
                humantime::format_rfc3339_seconds(std::time::SystemTime::now()),
                colors.color(record.level()),
                record.target(),
                message
            ))
        })
        .level(log::LevelFilter::Debug)
        .chain(std::io::stderr())
}
