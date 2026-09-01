use std::time::Duration;

const IO_TIMEOUT: Duration = Duration::from_secs(2);

#[cfg(target_os = "linux")]
pub fn socket_name() -> String {
    let uid = unsafe { libc::getuid() };
    match std::env::var("SKWD_WALL_V2_INSTANCE") {
        Ok(instance) if !instance.is_empty() => format!("skwd-wall-v2.{uid}.{instance}"),
        _ => format!("skwd-wall-v2.{uid}"),
    }
}

#[cfg(not(target_os = "linux"))]
pub fn socket_name() -> String {
    String::new()
}

#[cfg(target_os = "linux")]
pub fn send_command(command: &str) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::linux::net::SocketAddrExt;
    use std::os::unix::net::{SocketAddr, UnixStream};

    let address = SocketAddr::from_abstract_name(socket_name().as_bytes())?;
    let mut stream = UnixStream::connect_addr(&address)?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;
    stream.write_all(command.as_bytes())?;
    stream.write_all(b"\n")
}

#[cfg(not(target_os = "linux"))]
pub fn send_command(_command: &str) -> std::io::Result<()> {
    Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "linux only"))
}

#[cfg(target_os = "linux")]
pub fn send_query(command: &str) -> std::io::Result<String> {
    use std::io::{Read, Write};
    use std::os::linux::net::SocketAddrExt;
    use std::os::unix::net::{SocketAddr, UnixStream};

    let address = SocketAddr::from_abstract_name(socket_name().as_bytes())?;
    let mut stream = UnixStream::connect_addr(&address)?;
    stream.set_read_timeout(Some(IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;
    stream.write_all(command.as_bytes())?;
    stream.write_all(b"\n")?;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    Ok(response.trim_end().to_string())
}

#[cfg(not(target_os = "linux"))]
pub fn send_query(_command: &str) -> std::io::Result<String> {
    Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "linux only"))
}

#[cfg(test)]
#[path = "picker_control_tests.rs"]
mod tests;
