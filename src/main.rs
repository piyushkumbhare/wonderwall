#![allow(unused)]

use std::{
    io,
    process::{Command, Stdio},
    time::Duration,
};

use clap::Parser;

mod server;
use server::*;

mod utils;

/// A horribly written wallpaper engine
#[derive(Parser)]
struct Args {
    /// Start the Wallpaper server in the background
    #[arg(long = "start-server")]
    start_server: Option<String>,

    /// Runs the Wallpaper server in the current terminal
    #[arg(requires = "start_server", long = "run-here", default_value_t = false)]
    run_here: bool,

    /// Show all logs
    #[arg(short, long, default_value_t = false)]
    verbose: bool,

    /// Time (in seconds) between wallpaper switches
    #[arg(short, long, default_value_t = 300)]
    duration: u64,
}

fn main() {
    let args = Args::parse();

    // Enable logging if verbose
    if args.verbose {
        env_logger::Builder::new()
            .filter_level(log::LevelFilter::Debug)
            .init();
    }

    if args.start_server.is_some() {
        let directory = args.start_server.unwrap();
        match args.run_here {
            // Starts continuous server. If any error was encountered during setup,
            // the message will have been logged, so here we can just exit(1);
            true => {
                let mut server = WallpaperServer::new(directory);
                match server.start() {
                    Ok(_) => {}
                    Err(e) => {
                        log::error!("Server ran into error: {e}");
                        std::process::exit(1);
                    }
                }
            }
            false => {
                log::info!("Spawning server child process...");
                let mut child = Command::new("setsid")
                    .arg(std::env::args().next().unwrap())
                    .arg("--start-server")
                    .arg(directory)
                    .arg("--run-here")
                    .stdin(Stdio::null())
                    .stdout(Stdio::null()) // TODO: Redirect child process' stdout and stderr to a log file
                    .stderr(Stdio::null()) // TODO: For said log file, make a default that can be changed via command line args
                    .spawn()
                    .expect("Failed to start server child process");

                // Wait 1 second to see if the child runs into an error (most notably the port being in use already)
                std::thread::sleep(Duration::from_secs(1));
                match child.try_wait() {
                    Ok(status) => match status {
                        Some(status) => {
                            if !status.code().is_some_and(|c| c == 0) {
                                log::error!("There was a problem starting the server. Please check the logs to view the problem.")
                            } else {
                                log::error!("The server seems to have exitd successfully. Not sure why this happened, but we'll take it.")
                            }
                        }
                        None => {
                            log::info!("Server child process (most likely) spawned successfully!")
                        }
                    },
                    Err(e) => {
                        log::error!("Ran into unexpected error: {e}");
                    }
                }
            }
        }
    }
}
