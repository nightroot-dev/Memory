//! Blocking-ish TCP client for the sidecar (used by the gateway).
//!
//! One request per connection (the sidecar also speaks multi-line, but a
//! fresh connection per call keeps client state trivial). Timeouts are
//! explicit: callers treat ANY error as "sidecar down" and degrade.

use anyhow::{Result, Context};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use crate::protocol::{EmbedRequest, EmbedResponse};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const IO_TIMEOUT: Duration = Duration::from_secs(10);

pub struct SidecarClient {
    addr: String,
}

impl SidecarClient {
    pub fn new(addr: impl Into<String>) -> Self {
        Self { addr: addr.into() }
    }

    pub fn embed(&self, texts: Vec<String>) -> Result<EmbedResponse> {
        let sock_addr = self
            .addr
            .to_socket_addrs()
            .map_err(|e| anyhow::anyhow!("sidecar resolve {}: {e}", self.addr))?
            .next()
            .ok_or_else(|| anyhow::anyhow!("sidecar unresolvable: {}", self.addr))?;
        let stream = TcpStream::connect_timeout(&sock_addr, CONNECT_TIMEOUT)
            .map_err(|e| anyhow::anyhow!("sidecar connect {}: {e}", self.addr))?;
        stream.set_read_timeout(Some(IO_TIMEOUT))?;
        stream.set_write_timeout(Some(IO_TIMEOUT))?;
        let mut reader = BufReader::new(stream.try_clone()?);
        let mut writer = stream;
        let line = serde_json::to_string(&EmbedRequest { embed: texts })?;
        writer.write_all(line.as_bytes())?;
        writer.write_all(b"\n")?;
        writer.flush()?;
        let mut out = String::new();
        reader.read_line(&mut out)?;
        serde_json::from_str(&out).with_context(|| "sidecar bad response")
    }
}
