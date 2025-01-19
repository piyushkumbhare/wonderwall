use std::{
    fs::read_dir,
    io::{self},
    path::PathBuf,
    process::exit,
    sync::{Arc, Condvar, Mutex},
};

use clap::Parser;
use signal_hook::{consts::SIGUSR1, iterator::Signals};

/// A horribly written wallpaper engine
#[derive(Parser)]
struct Args {
    /// Directory to cycle images from
    directory: Option<PathBuf>,

    /// Show all logs
    #[arg(short, long, default_value_t = false)]
    verbose: bool,

    /// Time (in seconds) between wallpaper switches
    #[arg(short, long, default_value_t = 300)]
    duration: u64,
}

fn exec_command(command: &str) -> io::Result<String> {
    dbg!(&command);
    let output = std::process::Command::new("bash")
        .arg("-c")
        .arg(command)
        .output()?;

    Ok(output
        .stderr
        .iter()
        .map(|c| char::from(*c))
        .collect::<String>())
}

fn main() {
    let args = Args::parse();

    if args.verbose {
        env_logger::Builder::new()
            .filter_level(log::LevelFilter::Debug)
            .init();
    }

    let directory = match args.directory {
        Some(s) => PathBuf::from(s),
        None => PathBuf::from("/home/piyushk/Pictures/Backgrounds/"),
    };

    log::info!("Using pictures from {}", directory.to_string_lossy());

    // Refresh visible wallpapers
    let first_wallpaper: String = read_dir(&directory.canonicalize().unwrap())
        .unwrap_or_else(|_| {
            log::error!(
                "{} directory not found, exiting...",
                directory.to_string_lossy()
            );
            exit(1);
        })
        .find(|e| e.is_ok())
        .expect("Backgrounds Directory is empty!")
        .unwrap()
        .path()
        .to_string_lossy()
        .to_string();

    let trigger = Arc::new((Mutex::new(false), Condvar::new()));
    let trigger_clone = trigger.clone();

    let mut wp_index: usize = 0;

    let tmp_file = "/tmp/cycle-wallpaper.next";

    std::fs::write(tmp_file, first_wallpaper).unwrap();

    /*
        Child thread manages cycling of wallpapers
        Cycles and sleeps once every `duration` seconds OR if triggered by SIGUSR1 via main thread
    */
    std::thread::spawn(move || loop {
        // Acquire lock and sleep until notified OR <DURATION> seconds have elapsed
        let (lock, cvar) = &*trigger_clone;
        let triggered = lock.lock().unwrap();
        let _ = cvar.wait_timeout(triggered, std::time::Duration::from_secs(args.duration));

        log::info!("Changing wallpaper now!");

        // Load wallpaper from temp file
        let next_wallpaper: String = std::fs::read(tmp_file)
            .unwrap()
            .iter()
            .map(|c| char::from(*c))
            .collect();

        // Preloads image into memory for Hyprpaper
        let command = format!("hyprctl hyprpaper preload {}", next_wallpaper);
        let Ok(preload) = exec_command(&command) else {
            log::error!("Error executing command: `{command}`");
            exit(1);
        };
        log::debug!("{}", preload);

        // Loads the wallpaper
        let command = format!("hyprctl hyprpaper wallpaper \', {}\'", next_wallpaper);
        let Ok(load) = exec_command(&command) else {
            log::error!("Error executing command: `{command}`");
            exit(1);
        };
        log::debug!("{}", load);

        // Unloads all unused wallpapers out of memory
        let command = format!("hyprctl hyprpaper unload unused");
        let Ok(unload_unused) = exec_command(&command) else {
            log::error!("Error executing command: `{command}`");
            exit(1);
        };
        log::debug!("{}", unload_unused);

        // Refresh visible wallpapers
        let wallpapers: Vec<_> = read_dir(&directory.canonicalize().unwrap())
            .unwrap_or_else(|_| {
                log::error!(
                    "{} directory not found, exiting...",
                    directory.to_string_lossy()
                );
                exit(1);
            })
            .filter_map(|e| e.ok())
            .filter_map(|e| e.path().to_str().map(|p| p.to_string()))
            .collect();

        wp_index += 1;

        if wp_index >= wallpapers.len() {
            wp_index = 0;
        }

        std::fs::write(tmp_file, wallpapers[wp_index].clone()).unwrap();
    });

    /*
        Main thread manages signal handling
        Upon SIGUSR1, notify thread to update wallpaper and restart timer
    */
    let mut signals = Signals::new(&[SIGUSR1]).expect("Error setting up signals.");

    for signal in signals.forever() {
        match signal {
            SIGUSR1 => {
                let (lock, cvar) = &*trigger;
                {
                    let mut triggered = lock.lock().unwrap();
                    *triggered = true;
                }
                cvar.notify_one();
            }
            _ => log::error!("Got unregistered signal!"),
        }
    }
}
