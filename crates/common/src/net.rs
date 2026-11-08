//! Length-prefixed bincode framing over any byte stream.

use serde::{Serialize, de::DeserializeOwned};
use std::io::{self, Read, Write};

const MAX_FRAME: usize = 8 << 20;

pub fn write_frame<T: Serialize, W: Write>(w: &mut W, msg: &T) -> io::Result<()> {
    let body = bincode::serialize(msg).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    w.write_all(&(body.len() as u32).to_le_bytes())?;
    w.write_all(&body)?;
    Ok(())
}

pub fn encode_frame<T: Serialize>(msg: &T) -> Vec<u8> {
    let body = bincode::serialize(msg).expect("serialize");
    let mut out = Vec::with_capacity(body.len() + 4);
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    out
}

pub fn read_frame<T: DeserializeOwned, R: Read>(r: &mut R) -> io::Result<T> {
    let mut len = [0u8; 4];
    r.read_exact(&mut len)?;
    let len = u32::from_le_bytes(len) as usize;
    if len > MAX_FRAME {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "frame too large"));
    }
    let mut body = vec![0u8; len];
    r.read_exact(&mut body)?;
    bincode::deserialize(&body).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::ToServer;

    #[test]
    fn frames_round_trip() {
        let mut buf = Vec::new();
        write_frame(&mut buf, &ToServer::Chat("hi".into())).unwrap();
        write_frame(&mut buf, &ToServer::SetBlock { pos: [1, 2, 3], block: 4 }).unwrap();
        let mut r = &buf[..];
        assert!(matches!(read_frame::<ToServer, _>(&mut r).unwrap(), ToServer::Chat(s) if s == "hi"));
        assert!(matches!(read_frame::<ToServer, _>(&mut r).unwrap(), ToServer::SetBlock { pos: [1, 2, 3], block: 4 }));
    }
}
