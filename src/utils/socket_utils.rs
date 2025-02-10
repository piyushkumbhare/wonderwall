use std::{
    collections::HashMap,
    error::Error,
    fmt::Display,
    io::{self, BufRead, Write},
    os::unix::net::UnixStream,
};

use regex::Regex;

#[derive(Debug)]
pub struct PacketError<'a>(pub &'a str);

impl Display for PacketError<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl Error for PacketError<'_> {}

#[derive(Debug)]
pub struct Packet {
    pub method: String,
    pub headers: HashMap<String, String>,
    pub body: String,
}

impl Packet {
    pub fn new() -> Self {
        Self {
            method: String::from("POST /"),
            headers: HashMap::new(),
            body: String::from(""),
        }
    }

    /// Sets method
    pub fn method(mut self, method: &str) -> Self {
        self.method = format!("HTTP/1.1 {}", method.trim());
        self
    }

    /// Sets header
    pub fn header(mut self, key: &str, value: &str) -> Self {
        self.headers
            .insert(key.trim().to_string(), value.trim().to_string());
        self
    }

    /// Sets body
    pub fn body(mut self, body: &str) -> Self {
        self.body = body.to_string();
        self
    }

    /// Deserializes the packet from raw bytes
    pub fn from_bytes<'a>(buffer: Vec<u8>) -> Result<Self, PacketError<'a>> {
        let re = Regex::new(r#"^([^\r\n]+)\r\n((.+: .+\r\n)*)\r\n(.*)"#).unwrap();

        let buffer = String::from_utf8(buffer).unwrap();
        let Some(caps) = re.captures(&buffer) else {
            return Err(PacketError("Bad format"));
        };

        let method = match caps.get(1) {
            Some(s) => s.as_str(),
            None => return Err(PacketError("Bad method format")),
        }
        .to_string();

        let mut headers = HashMap::new();
        if let Some(h) = caps.get(2) {
            for line in h.as_str().split("\r\n") {
                if line.is_empty() {
                    continue;
                }
                let (key, value) = match line.split_once(": ") {
                    Some(kv) => kv,
                    None => return Err(PacketError("Bad header format")),
                };
                headers.insert(
                    key.trim().to_string(),
                    value.trim().trim_end_matches(",").to_string(),
                );
            }
        };

        let body = match caps.get(caps.len() - 1) {
            Some(b) => b.as_str(),
            None => "",
        }
        .to_string();

        Ok(Packet {
            method,
            headers,
            body,
        })
    }

    /// Serializes the packet into bytes
    pub fn as_bytes(&self) -> Vec<u8> {
        let mut output_buffer = String::new();

        output_buffer.push_str(format!("{} HTTP/1.1\r\n", &self.method).as_str());

        for (key, value) in self.headers.iter() {
            output_buffer.push_str(format!("{key}: {value}\r\n").as_str());
        }
        output_buffer.push_str("\r\n");

        output_buffer.push_str(&self.body);

        output_buffer.into()
    }
}

pub fn send_request(command: &str, body: &str, address: &str) -> Result<String, Box<dyn Error>> {
    let mut stream = UnixStream::connect(address)?;

    let request = Packet::new().header("WallpaperControl", command).body(body);
    stream.write_all(&request.as_bytes())?;
    stream.flush()?;

    let response_bytes = extract_bytes_buffered(&mut stream)?;
    let response = Packet::from_bytes(response_bytes)?;

    Ok(response.body)
}

/// Given a buffer (in this case, File socketStream), use `BufReader` and `BufRead` trait
/// to read the pending bytes in the stream
///
/// HOLY CRAP THANK YOU WHOEVER WROTE THIS, TOOK FOREVER TO WORK T_T
///
/// https://github.com/thepacketgeek/rust-tcpstream-demo/blob/master/raw/src/lib.rs
pub fn extract_bytes_buffered(mut buf: &mut impl io::Read) -> io::Result<Vec<u8>> {
    let mut reader = io::BufReader::new(&mut buf);

    // `fill_buf` will return a ref to the bytes pending (received by File socket)
    // This is still a lower-level call, so we have to follow it up with a call to consume
    let received: Vec<u8> = reader.fill_buf()?.to_vec();

    // Mark the bytes read as consumed so the buffer will not return them in a subsequent read
    reader.consume(received.len());

    Ok(received)
}
