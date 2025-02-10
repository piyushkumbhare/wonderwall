use clap::{ArgGroup, Parser};

#[derive(Debug)]
pub enum Action {
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
pub struct Args {
    /// The wallpaper to immediately set
    #[arg(short, long)]
    pub update: Option<String>,

    /// Cycles to the next wallpaper
    #[arg(short, long, default_value_t = false)]
    pub next: bool,

    /// Gets the directory the engine is currently cycling through
    #[arg(short, long = "get-dir")]
    pub get_dir: bool,

    /// Sets the directory the engine should cycle through
    #[arg(short, long = "set-dir")]
    pub set_dir: Option<String>,

    /// Start the Wallpaper server in the background
    #[arg(long = "start")]
    pub directory: Option<String>,

    /// Runs the Wallpaper server in the current terminal
    #[arg(
        requires = "directory",
        short,
        long = "run-here",
        default_value_t = false
    )]
    pub run_here: bool,

    // Kills the currently running server
    #[arg(short, long, default_value_t = false)]
    pub kill: bool,

    /* DEFAULT VALUE GLOBAL PARAMETERS */
    /// Show all logs
    #[arg(short, long, default_value_t = false)]
    pub verbose: bool,

    /// Time (in seconds) between wallpaper switches
    #[arg(short, long, default_value_t = 300)]
    pub duration: u64,
}

impl Args {
    pub fn action(&self) -> Action {
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
