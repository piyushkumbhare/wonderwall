#![allow(unused)]

use std::{
    io::{self, Write},
    net::TcpStream,
    process::{Command, Stdio},
    time::Duration,
};

use clap::{ArgGroup, Parser};

mod server;
use server::*;

mod file_utils;
mod packet_utils;

/// A horribly written wallpaper engine with an unreasonably good name
#[derive(Debug, Parser)]
#[clap(group(
    ArgGroup::new("action")
    .required(true)
    .args(&["directory", "wallpaper", "next", "get_dir", "set_dir", "kill"])
))]
struct Args {
    /// The wallpaper to immediately set
    #[arg(short, long)]
    wallpaper: Option<String>,

    /// Cycles to the next wallpaper
    #[arg(short, long, default_value_t = false)]
    next: bool,

    /// Gets the directory the engine is currently cycling through
    #[arg(short, long = "get-dir")]
    get_dir: bool,

    /// Sets the directory the engine should cycle through
    #[arg(short, long = "set-dir")]
    set_dir: Option<String>,

    /// Start the Wallpaper server in the background
    #[arg(long = "start")]
    directory: Option<String>,

    /// Runs the Wallpaper server in the current terminal
    #[arg(
        requires = "directory",
        short,
        long = "run-here",
        default_value_t = false
    )]
    run_here: bool,

    // Kills the currently running server
    #[arg(short, long, default_value_t = false)]
    kill: bool,

    /* DEFAULT VALUE GLOBAL PARAMETERS */
    /// Sets the address of the server
    #[arg(short, long = "addr", default_value_t = String::from("127.0.0.1"))]
    address: String,

    /// Sets the port of the server
    #[arg(short, long = "port", default_value_t = 6969)]
    port: u64,

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

    if args.directory.is_some() {
        let directory = args.directory.unwrap();

        match args.run_here {
            // Starts continuous server. If any error was encountered during setup,
            // the message will have been logged, so here we can just exit(1);
            true => {
                let mut server = WallpaperServer::new(
                    directory,
                    args.duration,
                    format!("{}:{}", args.address, args.port),
                )
                .unwrap();
                match server.start() {
                    Ok(_) => {}
                    Err(e) => {
                        log::error!("Server ran into error: {e}");
                        std::process::exit(1);
                    }
                }
            }
            false => {
                // TODO: Find a better way to implement background processes and disowning
                log::info!("Spawning server child process...");
                let mut child = Command::new("setsid")
                    .arg(std::env::args().next().unwrap())
                    .arg("--start")
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
                                eprintln!("There was a problem starting the server. This usually means the server is already running or the port is in use. Please check the logs to view the problem.")
                            } else {
                                eprintln!("The server seems to have exited successfully. Not sure why this happened, but we'll take it.")
                            }
                        }
                        None => {
                            eprintln!("Server child process (most likely) spawned successfully!")
                        }
                    },
                    Err(e) => {
                        eprintln!("Ran into unexpected error: {e}");
                    }
                }
            }
        }
        std::process::exit(0);
    }

    let address = format!("{}:{}", args.address, args.port);

    if args.wallpaper.is_some() {
        match packet_utils::send_request("Update", Some(args.wallpaper.clone().unwrap()), &address)
        {
            Ok(response) => println!("{response}"),
            Err(e) => {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        }
    }

    if args.next {
        match packet_utils::send_request("Cycle", None, &address) {
            Ok(response) => println!("{response}"),
            Err(e) => {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        }
    }

    if args.get_dir {
        match packet_utils::send_request("GetDir", None, &address) {
            Ok(response) => println!("{response}"),
            Err(e) => {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        }
    }

    if args.set_dir.is_some() {
        match packet_utils::send_request("SetDir", Some(args.set_dir.clone().unwrap()), &address) {
            Ok(response) => println!("{response}"),
            Err(e) => {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        }
    }

    if args.kill {
        match packet_utils::send_request("Stop", None, &address) {
            Ok(response) => println!("{response}"),
            Err(e) => {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        }
    }
}
