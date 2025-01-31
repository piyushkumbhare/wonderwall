use std::{
    collections::HashMap,
    error::Error,
    fmt::Display,
    io::{self, BufRead, Write},
    net::TcpStream,
    process::Output,
};

pub struct WallpaperPacket {
    pub headers: HashMap<String, String>,
    pub body: String,
}

#[derive(Debug)]
pub struct PacketError<'a>(pub &'a str);

impl<'a> Display for PacketError<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl<'a> Error for PacketError<'a> {}

#[derive(Debug)]
pub struct ServerError<'a>(pub &'a str);

impl<'a> Display for ServerError<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl<'a> Error for ServerError<'a> {}

/// Decodes a TCP packet, adhering to the HTTP protocol (or at least I hope so)
pub fn decode_packet<'a>(packet_buffer: Vec<u8>) -> Result<WallpaperPacket, PacketError<'a>> {
    // Split the packet contents at \r\n\r\n to separate headers from body
    let (headers_raw, body_raw) = match packet_buffer
        .windows(4)
        .position(|bytes| bytes == b"\r\n\r\n")
    {
        Some(i) => {
            let (headers, body) = packet_buffer.split_at(i);
            (headers, &body[4..])
        }
        None => return Err(PacketError("Packet has bad format.")),
    };

    let Ok(body) = String::from_utf8(Vec::from(body_raw)) else {
        return Err(PacketError(
            "Packet body could not be decoded into a string.",
        ));
    };
    let Ok(headers_string) = String::from_utf8(Vec::from(headers_raw)) else {
        return Err(PacketError(
            "Packet headers could not be decoded into a string.",
        ));
    };

    // Create headers HashMap
    let mut headers = HashMap::new();
    let Some((_method, header_iter)) = headers_string.split_once("\r\n") else {
        return Err(PacketError("Bad Method/Headers format."));
    };

    // Populate headers map with "key: value" pairs
    for header in header_iter.split("\r\n") {
        let Some((key, value)) = header.split_once(": ") else {
            return Err(PacketError("Bad headers format."));
        };
        headers.insert(key.to_string(), value.to_string());
    }

    Ok(WallpaperPacket { headers, body })
}

/// Builts a packet with standard, yet minimal HTTP headers along with an optional body
pub fn build_packet(successful: bool, body: Option<String>) -> String {
    let status_line = if successful {
        "200 OK"
    } else {
        "400 Bad Request"
    };
    let body = body.unwrap_or(String::from(""));
    format!(
        "HTTP/1.1 {}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{}",
        status_line,
        body.len(),
        body
    )
}

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

    let unload_unused = format!("hyprctl hyprpaper unload unused");
    let stdout = exec_command(&unload_unused)?;
    if stdout != "ok\n" {
        return Err(Box::new(HyprpaperError(stdout)));
    }
    Ok(())
}

pub fn exec_command(command: &str) -> io::Result<String> {
    let output = std::process::Command::new("bash")
        .arg("-c")
        .arg(command)
        .output()?;

    Ok(output.stdout.iter().map(|&c| char::from(c)).collect())
}

#[test]
fn test_hyprpaper() {
    let result = hyprpaper_update("~/Pictures/Backgrounds/ssgvegeta.png").unwrap();
}

/// Helper function to send an empty TCP response with Status 400
pub fn send_empty_response(mut stream: &TcpStream) {
    let response = build_packet(false, None);
    log::info!("Replying with packet:\n{response}");
    match stream.write_all(response.as_bytes()) {
        Ok(_) => {}
        Err(e) => {
            log::error!("Error when attempting to write to TCP stream: {e}");
        }
    }
}

/// Given a buffer (in this case, TcpStream), use `BufReader` and `BufRead` trait
/// to read the pending bytes in the stream
///
/// HOLY CRAP THANK YOU WHOEVER WROTE THIS, TOOK FOREVER TO WORK T_T
///
/// Credits: https://github.com/thepacketgeek/rust-tcpstream-demo/blob/master/raw/src/lib.rs
pub fn extract_string_buffered(mut buf: &mut impl io::Read) -> io::Result<Vec<u8>> {
    let mut reader = io::BufReader::new(&mut buf);

    // `fill_buf` will return a ref to the bytes pending (received by TCP)
    // This is still a lower-level call, so we have to follow it up with a call to consume
    let received: Vec<u8> = reader.fill_buf()?.to_vec();

    // Mark the bytes read as consumed so the buffer will not return them in a subsequent read
    reader.consume(received.len());

    Ok(received)
}
