//! The bounded RESP2 subset used by Redis pub/sub. No Redis dependency.
use std::io::{BufRead, Read};

const MAX_BULK: usize = 16 * 1024 * 1024;
const MAX_ITEMS: usize = 1024 * 1024;
const MAX_LINE: usize = 16 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reply {
    Simple(String),
    Error(String),
    Int(i64),
    Bulk(Option<Vec<u8>>),
    Array(Option<Vec<Reply>>),
}

pub fn encode(args: &[&[u8]]) -> Vec<u8> {
    let mut out = format!("*{}\r\n", args.len()).into_bytes();
    for arg in args {
        out.extend_from_slice(format!("${}\r\n", arg.len()).as_bytes());
        out.extend_from_slice(arg);
        out.extend_from_slice(b"\r\n");
    }
    out
}

fn exact(r: &mut impl Read, bytes: &mut [u8]) -> Result<(), String> {
    r.read_exact(bytes).map_err(|e| format!("RESP truncated or unreadable: {e}"))
}

fn line(r: &mut impl BufRead) -> Result<String, String> {
    let mut bytes = Vec::new();
    loop {
        let mut b = [0];
        exact(r, &mut b)?;
        if b[0] == b'\n' {
            if bytes.pop() != Some(b'\r') {
                return Err("RESP line needs CRLF".into());
            }
            return String::from_utf8(bytes).map_err(|_| "RESP line is not UTF-8".into());
        }
        if bytes.len() == MAX_LINE {
            return Err("RESP line is too long".into());
        }
        bytes.push(b[0]);
    }
}

fn integer(r: &mut impl BufRead) -> Result<i64, String> {
    let s = line(r)?;
    let digits = s.strip_prefix('-').unwrap_or(&s);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err("RESP invalid integer".into());
    }
    s.parse().map_err(|_| "RESP integer overflow".into())
}

/// Parse one complete reply, including aggregate item/byte limits. A timeout
/// midway through a frame is an error: callers must discard that connection.
pub fn read_reply(r: &mut impl BufRead) -> Result<Reply, String> {
    let (mut items, mut bytes) = (MAX_ITEMS, MAX_BULK);
    parse(r, 0, &mut items, &mut bytes)
}

fn parse(
    r: &mut impl BufRead,
    depth: usize,
    items: &mut usize,
    bytes: &mut usize,
) -> Result<Reply, String> {
    if depth > 8 {
        return Err("RESP nesting exceeds 8".into());
    }
    let mut kind = [0];
    exact(r, &mut kind)?;
    match kind[0] {
        b'+' | b'-' => {
            let text = line(r)?;
            *bytes = bytes.checked_sub(text.len()).ok_or("RESP aggregate byte limit")?;
            Ok(if kind[0] == b'+' { Reply::Simple(text) } else { Reply::Error(text) })
        }
        b':' => Ok(Reply::Int(integer(r)?)),
        b'$' => {
            let n = integer(r)?;
            if n == -1 {
                return Ok(Reply::Bulk(None));
            }
            let n = usize::try_from(n).map_err(|_| "RESP invalid bulk length")?;
            if n > MAX_BULK {
                return Err("RESP bulk exceeds 16 MiB".into());
            }
            *bytes = bytes.checked_sub(n).ok_or("RESP aggregate byte limit")?;
            let mut value = vec![0; n];
            exact(r, &mut value)?;
            let mut end = [0; 2];
            exact(r, &mut end)?;
            if &end != b"\r\n" {
                return Err("RESP bulk needs CRLF".into());
            }
            Ok(Reply::Bulk(Some(value)))
        }
        b'*' => {
            let n = integer(r)?;
            if n == -1 {
                return Ok(Reply::Array(None));
            }
            let n = usize::try_from(n).map_err(|_| "RESP invalid array length")?;
            *items = items.checked_sub(n).ok_or("RESP array exceeds 1 M items")?;
            let mut values = Vec::new();
            for _ in 0..n {
                values.push(parse(r, depth + 1, items, bytes)?);
            }
            Ok(Reply::Array(Some(values)))
        }
        _ => Err("RESP unknown reply type".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn read(bytes: &[u8]) -> Result<Reply, String> {
        read_reply(&mut std::io::Cursor::new(bytes))
    }

    #[test]
    fn all_reply_types_and_binary_encoding() {
        assert_eq!(read(b"+OK\r\n").unwrap(), Reply::Simple("OK".into()));
        assert_eq!(read(b"-ERR problem\r\n").unwrap(), Reply::Error("ERR problem".into()));
        assert_eq!(read(b":-12\r\n").unwrap(), Reply::Int(-12));
        assert_eq!(read(b"$-1\r\n").unwrap(), Reply::Bulk(None));
        assert_eq!(read(b"*-1\r\n").unwrap(), Reply::Array(None));
        assert_eq!(read(b"$0\r\n\r\n").unwrap(), Reply::Bulk(Some(vec![])));
        let args: &[&[u8]] = &[b"PUBLISH", b"topic", b"\0\r\n\xff"];
        assert_eq!(
            read(&encode(args)).unwrap(),
            Reply::Array(Some(args.iter().map(|a| Reply::Bulk(Some(a.to_vec()))).collect()))
        );
        assert_eq!(
            read(b"*1\r\n*1\r\n:2\r\n").unwrap(),
            Reply::Array(Some(vec![Reply::Array(Some(vec![Reply::Int(2)]))]))
        );
    }

    #[test]
    fn every_truncation_and_pubsub_frames() {
        for frame in [
            b"*3\r\n$9\r\nsubscribe\r\n$5\r\ntopic\r\n:1\r\n".as_slice(),
            &encode(&[b"message", b"topic", b"{}"]),
        ] {
            assert!(read(frame).is_ok());
            for n in 0..frame.len() {
                assert!(read(&frame[..n]).is_err(), "cut {n}");
            }
        }
    }

    #[test]
    fn refuses_bad_lengths_garbage_and_limits() {
        for bad in [
            b"?x\r\n".as_slice(),
            b"$-2\r\n",
            b"*-2\r\n",
            b"$16777217\r\n",
            b"*1048577\r\n",
            b":+1\r\n",
            b":\r\n",
            b":999999999999999999999\r\n",
            b"+OK\n",
            b"$1\r\naXX",
        ] {
            assert!(read(bad).is_err(), "{bad:?}");
        }
        assert!(read(format!("{}:1\r\n", "*1\r\n".repeat(9)).as_bytes()).is_err());
        assert!(read(format!("+{}\r\n", "x".repeat(MAX_LINE + 1)).as_bytes()).is_err());
    }
}
