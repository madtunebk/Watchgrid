//! Hidden password entry on the controlling terminal. Reads `/dev/tty`
//! directly, so passwords never come from arguments, environment
//! variables or pipes.

use std::fs::{File, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::os::fd::AsRawFd;

use zeroize::Zeroizing;

/// Restores the terminal's echo setting when dropped.
struct EchoOff<'a> {
    tty: &'a File,
    original: libc::termios,
}

impl Drop for EchoOff<'_> {
    fn drop(&mut self) {
        // SAFETY: valid fd and a termios value obtained from tcgetattr.
        unsafe { libc::tcsetattr(self.tty.as_raw_fd(), libc::TCSANOW, &self.original) };
    }
}

fn echo_off(tty: &File) -> io::Result<EchoOff<'_>> {
    let fd = tty.as_raw_fd();
    // SAFETY: termios is plain data; tcgetattr fills it for a valid fd.
    let mut term: libc::termios = unsafe { std::mem::zeroed() };
    if unsafe { libc::tcgetattr(fd, &mut term) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let guard = EchoOff { tty, original: term };
    term.c_lflag &= !libc::ECHO;
    term.c_lflag |= libc::ECHONL;
    // SAFETY: as above, with a modified copy.
    if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &term) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(guard)
}

pub fn read_hidden(prompt: &str) -> io::Result<Zeroizing<String>> {
    let tty = OpenOptions::new().read(true).write(true).open("/dev/tty").map_err(|e| io::Error::new(e.kind(), format!("a terminal is required to enter a password ({e})")))?;
    let _echo = echo_off(&tty)?;
    (&tty).write_all(prompt.as_bytes())?;
    (&tty).flush()?;
    let mut line = Zeroizing::new(String::new());
    BufReader::new(&tty).read_line(&mut line)?;
    let trimmed = line.trim_end_matches(['\r', '\n']);
    Ok(Zeroizing::new(trimmed.to_string()))
}
