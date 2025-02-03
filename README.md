# Wonderwall
This is a horrible wallpaper engine... but the name is wonderful (credits to LasagnaLord)

## Installation

```
git clone git@github.com:piyushkumbhare/wonderwall.git
```

```
cd wonderwall
```

```
cargo build --release
```
(or if you want to save some time typing, `cargo b -r`)

```
mv target/release/wonderwall ~/bin/
```

## Usage

```
$ wonderwall --help

A horribly written wallpaper engine with an unreasonably good name

Usage: wonderwall [OPTIONS] <--start <DIRECTORY>|--wallpaper <WALLPAPER>|--next|--get-dir|--set-dir <SET_DIR>|--kill>

Options:
  -w, --wallpaper <WALLPAPER>  The wallpaper to immediately set
  -n, --next                   Cycles to the next wallpaper
  -g, --get-dir                Gets the directory the engine is currently cycling through
  -s, --set-dir <SET_DIR>      Sets the directory the engine should cycle through
      --start <DIRECTORY>      Start the Wallpaper server in the background
  -r, --run-here               Runs the Wallpaper server in the current terminal
  -k, --kill
  -a, --addr <ADDRESS>         Sets the address of the server [default: 127.0.0.1]
  -p, --port <PORT>            Sets the port of the server [default: 6969]
  -v, --verbose                Show all logs
  -d, --duration <DURATION>    Time (in seconds) between wallpaper switches [default: 300]
  -h, --help                   Print help
```
