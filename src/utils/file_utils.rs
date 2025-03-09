use std::{
    error::Error,
    fmt::Display,
    io::{self},
    path::PathBuf,
};

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

    let unload_unused = "hyprctl hyprpaper unload unused";
    let stdout = exec_command(unload_unused)?;
    if stdout != "ok\n" {
        return Err(Box::new(HyprpaperError(stdout)));
    }
    Ok(())
}

pub fn exec_command(command: &str) -> io::Result<String> {
    log::info!("Executing command: `{}`", &command);
    let output = std::process::Command::new("bash")
        .arg("-c")
        .arg(command)
        .output()?;

    Ok(output.stdout.iter().map(|&c| char::from(c)).collect())
}

//pub fn get_directory_files(path: &str) -> io::Result<Vec<String>> {
//    let path = PathBuf::from(path).canonicalize()?;
//
//    let mut contents: Vec<_> = std::fs::read_dir(&path)?
//        .filter_map(|e| e.ok())
//        .filter_map(|e| {
//            let file_type = e.file_type().ok()?;
//            let is_file = file_type.is_file();
//            let path_str = e.path().to_str().map(|s| s.to_string());
//
//            // Log each entry's attributes if on debug mode
//            #[cfg(debug_assertions)]
//            {
//                log::debug!(
//                    "Found entry: {:?}, is_file: {}, path: {:?}",
//                    e.file_name(),
//                    is_file,
//                    path_str
//                );
//            }
//            is_file.then_some(path_str)?
//        })
//        .collect();
//
//    contents.sort();
//
//    Ok(contents)
//}

pub fn get_directory_files(path: &PathBuf, recursive: bool) -> io::Result<Vec<String>> {
    let path = PathBuf::from(path).canonicalize()?;
    let mut images: Vec<String> = vec![];

    for entry in std::fs::read_dir(&path)? {
        if let Ok(entry) = entry {
            if let Ok(file_type) = entry.file_type() {
                if file_type.is_file() {
                    if let Some(path) = entry.path().to_str() {
                        images.push(path.to_string())
                    }
                } else if file_type.is_dir() && recursive {
                    images.append(&mut get_directory_files(&entry.path(), true)?);
                }
            }
        }
    }
    Ok(images)
}

