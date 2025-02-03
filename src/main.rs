use std::{
    process::{Command, Stdio},
    time::Duration,
};

use clap::{ArgGroup, Parser};

mod server;
use server::*;

mod file_utils;
mod network_utils;

#[derive(Debug)]
enum Action {
    Update(String),
    Next,
    GetDir,
    SetDir(String),
    Kill,
}

/// A horribly written wallpaper engine with an unreasonably good name
#[derive(Debug, Parser, Clone)]
#[clap(group(
    ArgGroup::new("action")
    .required(true)
    .args(&["directory", "update", "next", "get_dir", "set_dir", "kill"])
))]
struct Args {
    /// The wallpaper to immediately set
    #[arg(short, long)]
    update: Option<String>,

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

impl Args {
    fn action(&self) -> Action {
        if let Some(ref update) = self.update {
            Action::Update(update.clone())
        } else if self.next {
            Action::Next
        } else if self.get_dir {
            Action::GetDir
        } else if let Some(ref dir) = self.set_dir {
            Action::SetDir(dir.clone())
        } else if self.kill {
            Action::Kill
        } else {
            unreachable!("Clap ensures one of the actions is always set");
        }
    }
}

fn main() {
    let args = Args::parse();

    // Enable logging if verbose
    if args.verbose {
        env_logger::Builder::new()
            .filter_level(log::LevelFilter::Debug)
            .init();
    }

    let address = format!("{}:{}", args.address, args.port);

    if args.directory.is_some() {
        start_server(args);
    } else {
        let request_result = match args.action() {
            Action::Update(wallpaper) => {
                network_utils::send_request("UPDATE", &wallpaper, &address)
            }
            Action::Next => network_utils::send_request("NEXT", "", &address),
            Action::GetDir => network_utils::send_request("GETDIR", "", &address),
            Action::SetDir(directory) => {
                network_utils::send_request("SETDIR", &directory, &address)
            }
            Action::Kill => network_utils::send_request("KILL", "", &address),
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

fn start_server(args: Args) {
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
                        if status.code().is_none_or(|c| c == 0) {
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
