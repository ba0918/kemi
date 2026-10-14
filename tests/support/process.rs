//! 子プロセスの出力を待機と並行して回収し、待ちに期限を設ける。

use std::io::{self, BufRead, BufReader, Cursor, Read};
use std::process::{Child, ExitStatus};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

const TIMEOUT: Duration = Duration::from_secs(20);

pub struct CapturedPipe {
    reader: BufReader<Chunks>,
}

impl CapturedPipe {
    pub fn new(mut pipe: impl Read + Send + 'static) -> Self {
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let mut buffer = [0; 8192];
            loop {
                match pipe.read(&mut buffer) {
                    Ok(0) => return,
                    Ok(read) => {
                        if sender.send(Ok(buffer[..read].to_vec())).is_err() {
                            return;
                        }
                    }
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(error) => {
                        let _ = sender.send(Err(error));
                        return;
                    }
                }
            }
        });
        Self {
            reader: BufReader::new(Chunks {
                receiver,
                current: Cursor::new(Vec::new()),
            }),
        }
    }
}

impl Read for CapturedPipe {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.reader.read(buffer)
    }
}

impl BufRead for CapturedPipe {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.reader.fill_buf()
    }

    fn consume(&mut self, amount: usize) {
        self.reader.consume(amount);
    }
}

struct Chunks {
    receiver: Receiver<io::Result<Vec<u8>>>,
    current: Cursor<Vec<u8>>,
}

impl Read for Chunks {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        if self.current.position() == self.current.get_ref().len() as u64 {
            self.current = match self.receiver.recv_timeout(TIMEOUT) {
                Ok(bytes) => Cursor::new(bytes?),
                Err(RecvTimeoutError::Disconnected) => return Ok(0),
                Err(RecvTimeoutError::Timeout) => {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "process output did not arrive",
                    ));
                }
            };
        }
        self.current.read(buffer)
    }
}

pub fn wait(child: &mut Child, timeout: Duration) -> io::Result<ExitStatus> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "process did not exit",
            ));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
