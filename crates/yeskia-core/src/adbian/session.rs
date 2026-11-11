use super::Command;
use crate::{operations::OperationContext, Error, Result};
use serde::Serialize;
use std::{io::Write, time::Duration};

pub const MAX_FRAME: usize = 512 * 1024;
pub const MAX_PUT: usize = 1024 * 1024;
pub const GET_CHUNK: usize = 256 * 1024;
const MAX_REPLY: usize = 8 * 1024 * 1024;
const SLICE: Duration = Duration::from_millis(100);

/// Partial I/O, bounded by timeout. Timeout may be retried only within the same operation;
/// a failed command or uncertain write is never replayed. close must release the transport.
pub trait Channel: Send {
    fn read(&mut self, bytes: &mut [u8], timeout: Duration) -> Result<usize>;
    fn write(&mut self, bytes: &[u8], timeout: Duration) -> Result<usize>;
    fn close(&mut self);
}

/// Explicit transient result: not journaled. Debug contains no remote data or messages.
#[derive(Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct Reply {
    pub ok: bool,
    pub text: String,
    pub data: Vec<u8>,
}
impl std::fmt::Debug for Reply {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Reply")
            .field("ok", &self.ok)
            .field("bytes", &self.data.len())
            .finish()
    }
}
impl Reply {
    pub fn require_ok(self) -> Result<Self> {
        if self.ok {
            Ok(self)
        } else {
            Err(Error::OperationFailed("ADBIAN_REMOTE_REFUSED".into()))
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stat {
    pub bytes: u64,
    pub is_dir: bool,
    pub revision: String,
}
fn invalid() -> Error {
    Error::Transport("ADBIAN_INVALID_FRAME".into())
}
fn path_arg(path: &str) -> Result<String> {
    crate::paths::validate_device_path(&path.replace('\\', "/"))?;
    if !path.as_bytes().get(1).is_some_and(|c| *c == b':') {
        return Err(Error::UnsafePath("ADBIAN_ABSOLUTE_PATH".into()));
    }
    Ok(path.replace('/', "\\"))
}

pub struct Session {
    channel: Box<dyn Channel>,
    healthy: bool,
}
impl Session {
    pub fn open(channel: Box<dyn Channel>, ctx: &OperationContext) -> Result<Self> {
        let mut s = Self {
            channel,
            healthy: true,
        };
        let r = s.reply(ctx)?;
        if !r.ok || r.text != "rshell ready" || !r.data.is_empty() {
            s.close();
            return Err(invalid());
        }
        Ok(s)
    }
    pub fn healthy(&self) -> bool {
        self.healthy
    }
    pub fn close(&mut self) {
        self.healthy = false;
        self.channel.close();
    }
    fn guard<T>(&mut self, r: Result<T>) -> Result<T> {
        if r.is_err() {
            self.close();
        }
        r
    }
    fn read_exact(&mut self, b: &mut [u8], ctx: &OperationContext) -> Result<()> {
        let mut pos = 0;
        while pos < b.len() {
            ctx.checkpoint()?;
            match self.channel.read(&mut b[pos..], SLICE) {
                Err(Error::Timeout) => continue,
                Ok(0) => return Err(Error::DeviceDisconnected),
                Ok(n) if n <= b.len() - pos => pos += n,
                Ok(_) => return Err(invalid()),
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
    fn write_all(&mut self, b: &[u8], ctx: &OperationContext) -> Result<()> {
        let mut pos = 0;
        while pos < b.len() {
            ctx.checkpoint()?;
            match self.channel.write(&b[pos..], SLICE) {
                Err(Error::Timeout) => continue,
                Ok(0) => return Err(Error::DeviceDisconnected),
                Ok(n) if n <= b.len() - pos => pos += n,
                Ok(_) => return Err(invalid()),
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
    fn send(&mut self, bytes: &[u8], ctx: &OperationContext) -> Result<()> {
        if !self.healthy {
            return Err(Error::NotConnected);
        }
        if bytes.len() > MAX_FRAME {
            return Err(Error::LimitExceeded("ADBIAN_FRAME_LIMIT".into()));
        }
        self.write_all(&(bytes.len() as u32).to_be_bytes(), ctx)?;
        self.write_all(bytes, ctx)
    }
    fn reply_inner(&mut self, ctx: &OperationContext) -> Result<Reply> {
        let mut data = Vec::new();
        for _ in 0..4096 {
            let mut header = [0; 4];
            self.read_exact(&mut header, ctx)?;
            let len = u32::from_be_bytes(header) as usize;
            if len == 0 || len > MAX_FRAME {
                return Err(invalid());
            }
            let mut frame = vec![0; len];
            self.read_exact(&mut frame, ctx)?;
            match frame[0] {
                b'.' => {
                    if data.len() + len - 1 > MAX_REPLY {
                        return Err(Error::LimitExceeded("ADBIAN_REPLY_LIMIT".into()));
                    }
                    data.extend_from_slice(&frame[1..]);
                }
                b'+' | b'-' => {
                    return Ok(Reply {
                        ok: frame[0] == b'+',
                        text: String::from_utf8(frame[1..].to_vec()).map_err(|_| invalid())?,
                        data,
                    })
                }
                _ => return Err(invalid()),
            }
        }
        Err(invalid())
    }
    fn reply(&mut self, ctx: &OperationContext) -> Result<Reply> {
        let r = self.reply_inner(ctx);
        self.guard(r)
    }
    pub fn command(&mut self, cmd: &Command, ctx: &OperationContext) -> Result<Reply> {
        let r = self.send(cmd.line().as_bytes(), ctx);
        self.guard(r)?;
        let reply = self.reply(ctx)?;
        if cmd.verb() == "quit" || (cmd.verb() == "reboot" && reply.ok) {
            self.close();
        }
        Ok(reply)
    }
    pub fn text_command(&mut self, line: &str, ctx: &OperationContext) -> Result<Reply> {
        self.command(&Command::parse(line)?, ctx)?.require_ok()
    }
    pub fn ping(&mut self, ctx: &OperationContext) -> Result<()> {
        let reply = self.text_command("ping", ctx)?;
        if reply.text != "pong" || !reply.data.is_empty() {
            self.close();
            return Err(invalid());
        }
        Ok(())
    }
    pub fn stat(&mut self, path: &str, ctx: &OperationContext) -> Result<Option<Stat>> {
        let path = path_arg(path)?;
        let r = self.command(&Command::parse(&format!("stat {path}"))?, ctx)?;
        if !r.ok {
            // Only the actual Symbian NotFound / PathNotFound codes mean absence.
            if r.text == "stat failed NotFound" || r.text == "stat failed PathNotFound" {
                return Ok(None);
            }
            return Err(Error::OperationFailed("ADBIAN_STAT_REFUSED".into()));
        }
        let rest = r
            .text
            .strip_prefix(&format!("{path}  "))
            .ok_or_else(invalid)?;
        let (n, revision) = rest.split_once(" bytes  ").ok_or_else(invalid)?;
        Ok(Some(Stat {
            bytes: n.parse().map_err(|_| invalid())?,
            is_dir: revision.ends_with("  <dir>"),
            revision: revision.into(),
        }))
    }
    pub fn read_to(
        &mut self,
        path: &str,
        sink: &mut dyn Write,
        ctx: &OperationContext,
    ) -> Result<u64> {
        let path = path_arg(path)?;
        let before = self.stat(&path, ctx)?.ok_or(Error::NotFoundOnSource)?;
        if before.is_dir {
            return Err(Error::InvalidArgument("ADBIAN_IS_DIRECTORY".into()));
        }
        let mut done = 0;
        while done < before.bytes {
            ctx.checkpoint()?;
            let mut want = (before.bytes - done).min(GET_CHUNK as u64);
            // Also compatible with older upstream split_range using first substring match.
            if done == want && want > 1 {
                want -= 1;
            }
            let r = self.text_command(&format!("cat {path} {done} {want}"), ctx)?;
            if r.data.len() as u64 != want {
                return Err(Error::VerificationFailed);
            }
            sink.write_all(&r.data)?;
            done += want;
            ctx.report_progress(done, before.bytes);
        }
        if self.stat(&path, ctx)?.as_ref() != Some(&before) {
            return Err(Error::VerificationFailed);
        }
        Ok(done)
    }
    pub fn put(&mut self, path: &str, data: &[u8], ctx: &OperationContext) -> Result<()> {
        let path = path_arg(path)?;
        if data.len() > MAX_PUT {
            return Err(Error::LimitExceeded("ADBIAN_PUT_1_MIB".into()));
        }
        if path.starts_with("Z:") || path.starts_with("z:") {
            return Err(Error::CapabilityUnsupported("ADBIAN_ROM_READ_ONLY".into()));
        }
        let r = self.put_inner(&path, data, ctx);
        self.guard(r)
    }
    fn put_inner(&mut self, path: &str, data: &[u8], ctx: &OperationContext) -> Result<()> {
        self.send(format!("put {path} {}", data.len()).as_bytes(), ctx)?;
        let ready = self.reply(ctx)?.require_ok()?;
        if !data.is_empty() {
            if ready.text != format!("send {} bytes", data.len()) || !ready.data.is_empty() {
                return Err(invalid());
            }
            let mut done = 0;
            for chunk in data.chunks(128 * 1024) {
                self.send(chunk, ctx)?;
                done += chunk.len();
                ctx.report_progress(done as u64, data.len() as u64);
            }
            self.reply(ctx)?.require_ok()?;
        }
        // Even a final OK only proves the command replied: caller/Engine performs read-back.
        Ok(())
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        self.close();
    }
}
