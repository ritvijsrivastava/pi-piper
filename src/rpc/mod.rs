//! JSONL line-protocol helpers shared by the `pi` RPC process bridge.
//!
//! pi's RPC mode (`pi --mode rpc`) communicates over stdin/stdout using one
//! JSON object per line, delimited strictly by `\n` (see pi's
//! `docs/rpc.md`). This module provides small helpers for that framing
//! without re-implementing anything `tokio::io::AsyncBufReadExt` already
//! gets right: it splits on `\n` only and strips a trailing `\r`, which
//! matches the RPC framing rules exactly. Pi Piper relays the same JSON
//! protocol to the browser, so no separate wire format needs to be
//! designed or kept in sync.

pub mod process;
pub mod remote_agent;

use anyhow::{Context, Result};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader, Lines};

/// Reads newline-delimited JSON values from an async byte stream (e.g. a
/// child process's stdout).
pub struct JsonLineReader<R> {
    lines: Lines<BufReader<R>>,
}

impl<R> JsonLineReader<R>
where
    R: AsyncRead + Unpin,
{
    /// Wraps `reader` for line-buffered JSON reading.
    pub fn new(reader: R) -> Self {
        Self {
            lines: BufReader::new(reader).lines(),
        }
    }

    /// Reads and parses the next JSON line.
    ///
    /// Returns `Ok(None)` on clean EOF (the underlying stream closed).
    /// A malformed (non-JSON) line is returned as an `Err`; the raw line
    /// is included in the error context so callers can log it and keep
    /// reading subsequent lines rather than treating it as fatal.
    pub async fn next(&mut self) -> Result<Option<Value>> {
        let Some(line) = self
            .lines
            .next_line()
            .await
            .context("reading line from stream")?
        else {
            return Ok(None);
        };

        let value =
            serde_json::from_str(&line).with_context(|| format!("invalid JSON line: {line}"))?;
        Ok(Some(value))
    }
}

/// Serializes `value` compactly and writes it as one JSONL record
/// (`<json>\n`), flushing immediately so the reader on the other end
/// (a child process or a browser socket) observes it right away.
pub async fn write_json_line<W>(writer: &mut W, value: &Value) -> Result<()>
where
    W: AsyncWrite + Unpin,
{
    let mut line = serde_json::to_string(value).context("serializing JSON line")?;
    line.push('\n');
    writer
        .write_all(line.as_bytes())
        .await
        .context("writing JSON line")?;
    writer.flush().await.context("flushing JSON line")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Cursor;

    #[tokio::test]
    async fn reads_multiple_lines_then_eof() {
        let data = b"{\"a\":1}\n{\"b\":2}\n".to_vec();
        let mut reader = JsonLineReader::new(Cursor::new(data));

        assert_eq!(reader.next().await.unwrap(), Some(json!({"a": 1})));
        assert_eq!(reader.next().await.unwrap(), Some(json!({"b": 2})));
        assert_eq!(reader.next().await.unwrap(), None);
    }

    #[tokio::test]
    async fn strips_trailing_carriage_return() {
        // RPC framing allows an optional trailing \r before the \n.
        let data = b"{\"a\":1}\r\n".to_vec();
        let mut reader = JsonLineReader::new(Cursor::new(data));

        assert_eq!(reader.next().await.unwrap(), Some(json!({"a": 1})));
    }

    #[tokio::test]
    async fn surfaces_malformed_line_but_allows_continuing() {
        let data = b"not json\n{\"ok\":true}\n".to_vec();
        let mut reader = JsonLineReader::new(Cursor::new(data));

        assert!(reader.next().await.is_err());
        assert_eq!(reader.next().await.unwrap(), Some(json!({"ok": true})));
    }

    #[tokio::test]
    async fn writes_json_followed_by_newline() {
        let mut buf: Vec<u8> = Vec::new();
        let value = json!({"type": "prompt", "message": "hi"});
        write_json_line(&mut buf, &value).await.unwrap();

        let written = String::from_utf8(buf).unwrap();
        assert!(written.ends_with('\n'), "line must end with a newline");
        // serde_json does not guarantee key order without the
        // `preserve_order` feature, so compare parsed values rather than
        // raw bytes.
        assert_eq!(
            serde_json::from_str::<Value>(written.trim_end()).unwrap(),
            value
        );
    }
}
