//! Encoded-data budgets, not a claim about exact process RSS. Fail closed; never truncate.
use anyhow::{Result, bail};
use serde::Serialize;
use std::io::{self, Write};

pub const CLI_BYTES: usize = 32 * 1024 * 1024;
pub const STDERR_BYTES: usize = 256 * 1024;
pub const ENTRY_BYTES: usize = 64 * 1024 * 1024;
pub const WORKSPACE_BYTES: usize = 64 * 1024 * 1024;
pub const CACHE_BYTES: u64 = 256 * 1024 * 1024;
pub const CACHE_ENTRIES: usize = 4096;
pub const RECORDS: usize = 20_000;

struct Budget<W> {
    writer: W,
    remaining: usize,
}
impl<W: Write> Write for Budget<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.remaining {
            return Err(io::Error::other("Encoded data exceeds limit"));
        }
        let n = self.writer.write(bytes)?;
        self.remaining -= n;
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}
pub fn encode(value: &impl Serialize, limit: usize) -> Result<Vec<u8>> {
    let mut writer = Budget {
        writer: Vec::new(),
        remaining: limit,
    };
    serde_json::to_writer(&mut writer, value)
        .map_err(|_| anyhow::anyhow!("Cannot encode private data within size limit"))?;
    Ok(writer.writer)
}
pub fn size(value: &impl Serialize, limit: usize) -> Result<usize> {
    let mut writer = Budget {
        writer: io::sink(),
        remaining: limit,
    };
    serde_json::to_writer(&mut writer, value)
        .map_err(|_| anyhow::anyhow!("Workspace exceeds encoded-data limit; narrow the scope"))?;
    Ok(limit - writer.remaining)
}
pub fn tickets(items: &[crate::data::Ticket]) -> Result<usize> {
    if items.len() > RECORDS {
        bail!("Workspace exceeds ticket-count limit; narrow the scope");
    }
    size(&items, WORKSPACE_BYTES)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn serialization_and_aggregate_limits_fail_without_truncation() {
        assert_eq!(encode(&"1234", 6).unwrap(), b"\"1234\"");
        assert!(encode(&"12345", 6).is_err());
        assert_eq!(size(&"1234", 6).unwrap(), 6);
        assert!(size(&vec!["1234"; 10], 20).is_err());
        assert!(tickets(&vec![crate::data::demo_tickets()[0].clone(); RECORDS + 1]).is_err());
    }
}
