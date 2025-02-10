# Wonderwall
This is an alright wallpaper engine... but the name is wonderful (credits to LasagnaLord)

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

An... okay wallpaper engine with an unreasonably good name

Usage: wonderwall [OPTIONS] <COMMAND>

Commands:
  start    Start the wallpaper server at a specified directory
  update   Manually update the wallpaper with a provided path
  next     Cycle to the next wallpaper in the queue
  get-dir  Print out the current wallpaper directory
  set-dir  Set the directory to cycle through
  ping     Ping the wallpaper server
  kill     Stop the wallpaper server
  help     Print this message or the help of the given subcommand(s)

Options:
  -v, --verbose  Show log info
  -h, --help     Print help

```
