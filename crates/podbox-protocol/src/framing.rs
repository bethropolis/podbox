use std::io::{self, Read, Write};

use serde::Serialize;

/// Write a length-prefixed JSON frame.
pub fn write_frame<W: Write>(w: &mut W, msg: &impl Serialize) -> io::Result<()> {
    let json = serde_json::to_vec(msg)?;
    if json.len() > MAX_FRAME_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "frame too large: {} bytes (max {MAX_FRAME_SIZE})",
                json.len()
            ),
        ));
    }
    let len = u32::try_from(json.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "frame payload exceeds 4 GiB"))?
        .to_be_bytes();
    w.write_all(&len)?;
    w.write_all(&json)?;
    w.flush()?;
    Ok(())
}

/// Maximum frame size: 16 MiB.
const MAX_FRAME_SIZE: usize = 16 * 1024 * 1024;

/// Read a length-prefixed JSON frame.
pub fn read_frame<R: Read>(r: &mut R) -> io::Result<Option<Vec<u8>>> {
    let mut len_buf = [0u8; 4];
    match r.read_exact(&mut len_buf) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > MAX_FRAME_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("frame too large: {len} bytes (max {MAX_FRAME_SIZE})"),
        ));
    }
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf)?;
    Ok(Some(buf))
}

/// Incremental frame reader that survives read timeouts.
///
/// [`read_frame`] uses `read_exact`, which discards bytes it already read when
/// a timeout fires mid-frame. The stream is then misaligned: the next read
/// parses payload bytes as a length prefix. `FrameReader` keeps the partial
/// frame, so a timeout returns `WouldBlock`/`TimedOut` with nothing lost and
/// the caller can retry, or give up once [`FrameReader::stalled_for`] is long.
#[derive(Debug, Default)]
pub struct FrameReader {
    prefix: [u8; 4],
    prefix_filled: usize,
    body: Vec<u8>,
    body_filled: usize,
    body_len: Option<usize>,
    /// When the first byte of the frame in progress arrived.
    started: Option<std::time::Instant>,
}

/// What a [`FrameReader::read`] call produced.
#[derive(Debug, PartialEq, Eq)]
pub enum FrameRead {
    /// A complete frame payload.
    Frame(Vec<u8>),
    /// The peer closed the stream at a frame boundary.
    Eof,
}

impl FrameReader {
    pub fn new() -> Self {
        Self::default()
    }

    /// True when part of a frame has been read and the rest is still due.
    pub fn in_progress(&self) -> bool {
        self.started.is_some()
    }

    /// Time since the first byte of the frame in progress, if one is in progress.
    pub fn stalled_for(&self) -> Option<std::time::Duration> {
        self.started.map(|t| t.elapsed())
    }

    /// Read until one frame is complete. A timeout returns `Err` and keeps the
    /// partial frame, so a later call resumes exactly where this one stopped.
    pub fn read<R: Read>(&mut self, r: &mut R) -> io::Result<FrameRead> {
        while self.prefix_filled < 4 {
            let n = r.read(&mut self.prefix[self.prefix_filled..])?;
            if n == 0 {
                return if self.prefix_filled == 0 {
                    Ok(FrameRead::Eof)
                } else {
                    Err(io::Error::from(io::ErrorKind::UnexpectedEof))
                };
            }
            self.started.get_or_insert_with(std::time::Instant::now);
            self.prefix_filled += n;
        }
        let len = match self.body_len {
            Some(len) => len,
            None => {
                let len = u32::from_be_bytes(self.prefix) as usize;
                if len > MAX_FRAME_SIZE {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("frame too large: {len} bytes (max {MAX_FRAME_SIZE})"),
                    ));
                }
                self.body = vec![0u8; len];
                self.body_len = Some(len);
                len
            }
        };
        while self.body_filled < len {
            let n = r.read(&mut self.body[self.body_filled..])?;
            if n == 0 {
                return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
            }
            self.body_filled += n;
        }
        let frame = std::mem::take(&mut self.body);
        *self = Self::default();
        Ok(FrameRead::Frame(frame))
    }
}
