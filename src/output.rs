use std::fs::{File, OpenOptions};
use std::io::{self, StdoutLock, Write};

pub enum Output {
    File(File),
    Stdout(StdoutLock<'static>),
}

impl Write for Output {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Output::File(f) => f.write(buf),
            Output::Stdout(s) => s.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Output::File(f) => f.flush(),
            Output::Stdout(s) => s.flush(),
        }
    }
}

pub fn create(path: Option<&str>) -> Result<Output, Box<dyn std::error::Error>> {
    Ok(match path {
        Some(p) => Output::File(OpenOptions::new().create(true).append(true).open(p)?),
        None => Output::Stdout(Box::leak(Box::new(io::stdout())).lock()),
    })
}
