use clap::{Parser, Subcommand};

#[derive(Clone, Debug, Subcommand)]
pub enum Opt {
    /// Start the wallpaper server at a specified directory
    Start {
        /// Directory containing wallpapers to cycle through
        directory: String,

        /// Time (in seconds) between automatic wallpaper updates
        #[arg(short, long, default_value_t = 600)]
        duration: u64,

        /// Runs the wallpaper server in the current terminal (useful for debugging)
        #[arg(short, long = "run-here", default_value_t = false)]
        run_here: bool,
    },

    /// Manually update the wallpaper with a provided path
    Update {
        /// Path to wallpaper
        path: String,
    },

    /// Cycle to the next wallpaper in the queue
    Next,

    /// Print out the current wallpaper directory
    GetDir,

    /// Set the directory to cycle through
    SetDir {
        /// Directory containing wallpapers to cycle through
        directory: String,
    },

    /// Ping the wallpaper server
    Ping,

    /// Stop the wallpaper server
    Kill,
}

/// A horribly written wallpaper engine with an unreasonably good name
#[derive(Debug, Parser, Clone)]
pub struct Args {
    #[command(subcommand)]
    pub command: Opt,

    /// Show log info
    #[arg(short, long, default_value_t = false)]
    pub verbose: bool,
}
