/// Static error message used when writing to socket stream fails.
pub const SOCKET_WRITE_ERROR: &str = "Failed to write to File Socket Stream!";

/// Default/built-in socket file path to use. Feel free to change this if you for some reason have one already
/// or if you don't want to keep it in `/tmp/`
pub const FILE_SOCKET: &str = "/tmp/wonderwall.sock";
