use std::io::{self, IsTerminal, Read, Write};
use std::sync::{Arc, Mutex};

pub struct IoStreams {
    stdin: Box<dyn Read + Send>,
    stdout: Box<dyn Write + Send>,
    stderr: Box<dyn Write + Send>,
    color_enabled: bool,
    is_stdout_tty: bool,
    is_stderr_tty: bool,
    is_stdin_tty: bool,
}

impl IoStreams {
    pub fn system() -> Self {
        let is_stdout_tty = io::stdout().is_terminal();
        let is_stderr_tty = io::stderr().is_terminal();
        let is_stdin_tty = io::stdin().is_terminal();
        Self {
            stdin: Box::new(io::stdin()),
            stdout: Box::new(io::stdout()),
            stderr: Box::new(io::stderr()),
            color_enabled: detect_color(is_stdout_tty),
            is_stdout_tty,
            is_stderr_tty,
            is_stdin_tty,
        }
    }

    pub fn test() -> (Self, TestBuffers) {
        let stdin = Arc::new(Mutex::new(Vec::<u8>::new()));
        let stdout = Arc::new(Mutex::new(Vec::<u8>::new()));
        let stderr = Arc::new(Mutex::new(Vec::<u8>::new()));
        let bufs = TestBuffers {
            stdin: stdin.clone(),
            stdout: stdout.clone(),
            stderr: stderr.clone(),
        };
        let io = Self {
            stdin: Box::new(SharedReader(stdin)),
            stdout: Box::new(SharedWriter(stdout)),
            stderr: Box::new(SharedWriter(stderr)),
            color_enabled: false,
            is_stdout_tty: false,
            is_stderr_tty: false,
            is_stdin_tty: false,
        };
        (io, bufs)
    }

    pub fn out(&mut self) -> &mut dyn Write {
        &mut *self.stdout
    }

    pub fn err(&mut self) -> &mut dyn Write {
        &mut *self.stderr
    }

    pub fn input(&mut self) -> &mut dyn Read {
        &mut *self.stdin
    }

    pub fn color_enabled(&self) -> bool {
        self.color_enabled
    }

    pub fn is_stdout_tty(&self) -> bool {
        self.is_stdout_tty
    }

    pub fn is_stderr_tty(&self) -> bool {
        self.is_stderr_tty
    }

    pub fn is_stdin_tty(&self) -> bool {
        self.is_stdin_tty
    }

    /// Pager support is post-MVP; left as a no-op so call sites compile.
    pub fn start_pager(&mut self) -> io::Result<()> {
        Ok(())
    }

    pub fn stop_pager(&mut self) {}
}

#[derive(Clone)]
pub struct TestBuffers {
    pub stdin: Arc<Mutex<Vec<u8>>>,
    pub stdout: Arc<Mutex<Vec<u8>>>,
    pub stderr: Arc<Mutex<Vec<u8>>>,
}

impl TestBuffers {
    pub fn stdout_string(&self) -> String {
        String::from_utf8(self.stdout.lock().unwrap().clone()).expect("stdout was not utf-8")
    }

    pub fn stderr_string(&self) -> String {
        String::from_utf8(self.stderr.lock().unwrap().clone()).expect("stderr was not utf-8")
    }
}

struct SharedWriter(Arc<Mutex<Vec<u8>>>);

impl Write for SharedWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct SharedReader(Arc<Mutex<Vec<u8>>>);

impl Read for SharedReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut guard = self.0.lock().unwrap();
        let n = buf.len().min(guard.len());
        buf[..n].copy_from_slice(&guard[..n]);
        guard.drain(..n);
        Ok(n)
    }
}

fn detect_color(is_stdout_tty: bool) -> bool {
    // NO_COLOR (https://no-color.org) disables color when set to any value.
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    // CLICOLOR_FORCE forces color even when stdout isn't a TTY.
    if std::env::var_os("CLICOLOR_FORCE").is_some_and(|v| v != "0") {
        return true;
    }
    is_stdout_tty
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_streams_capture_writes() {
        let (mut io, bufs) = IoStreams::test();
        write!(io.out(), "hello").unwrap();
        write!(io.err(), "warning").unwrap();
        assert_eq!(bufs.stdout_string(), "hello");
        assert_eq!(bufs.stderr_string(), "warning");
    }

    #[test]
    fn test_streams_have_color_disabled() {
        let (io, _) = IoStreams::test();
        assert!(!io.color_enabled());
        assert!(!io.is_stdout_tty());
    }

    #[test]
    fn test_streams_read_from_stdin_buffer() {
        let (mut io, bufs) = IoStreams::test();
        bufs.stdin.lock().unwrap().extend_from_slice(b"input bytes");
        let mut out = String::new();
        io.input().read_to_string(&mut out).unwrap();
        assert_eq!(out, "input bytes");
    }
}
