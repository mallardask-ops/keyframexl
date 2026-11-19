use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};

pub(crate) const VERSION: u32 = 1;
/// Plugin states can be large (sample-based instruments keep megabytes).
const MAX_MESSAGE: usize = 256 * 1024 * 1024;

#[derive(Debug, Serialize, Deserialize)]
pub(crate) enum Message {
    Hello {
        version: u32,
        generation: u64,
        rate: u32,
        max_frames: u32,
        max_events: u32,
    },
    Ready {
        version: u32,
        generation: u64,
    },
    /// What to host and where the shared audio block is (a file in /dev/shm).
    Load {
        mode: String,
        shm: String,
    },
    Loaded,
    Error {
        reason: String,
    },
    Shutdown,
    /// A plugin-host request, as the in-process host takes it.
    Req(crate::plugins::Req),
    /// Its answer (a loaded plugin's processor stays in the child).
    Reply(WireReply),
}

/// `plugins::Reply` without the processor, which cannot leave the child.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) enum WireReply {
    Loaded {
        id: u64,
        name: String,
        gui: bool,
        #[serde(default)]
        outs: usize,
    },
    Failed {
        id: u64,
        error: String,
    },
    Editor {
        id: u64,
        open: bool,
    },
    States(Vec<(u64, Vec<u8>)>),
    EditorSize {
        id: u64,
        w: i32,
        h: i32,
    },
    Params {
        id: u64,
        rows: Vec<crate::plugins::ParamRow>,
    },
    Bye,
}

pub(crate) fn write_message(w: &mut impl Write, message: &Message) -> io::Result<()> {
    let bytes = serde_json::to_vec(message).map_err(io::Error::other)?;
    if bytes.len() > MAX_MESSAGE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "message too long",
        ));
    }
    w.write_all(&(bytes.len() as u32).to_le_bytes())?;
    w.write_all(&bytes)
}

pub(crate) fn read_message(r: &mut impl Read) -> io::Result<Message> {
    let mut len = [0; 4];
    r.read_exact(&mut len)?;
    let len = u32::from_le_bytes(len) as usize;
    if len > MAX_MESSAGE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "message too long",
        ));
    }
    let mut bytes = vec![0; len];
    r.read_exact(&mut bytes)?;
    serde_json::from_slice(&bytes).map_err(io::Error::other)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn framing_round_trip_and_length_limit() {
        let mut bytes = Vec::new();
        write_message(
            &mut bytes,
            &Message::Hello {
                version: VERSION,
                generation: 7,
                rate: 48000,
                max_frames: 256,
                max_events: 128,
            },
        )
        .unwrap();
        assert!(matches!(
            read_message(&mut bytes.as_slice()).unwrap(),
            Message::Hello { generation: 7, .. }
        ));
        assert_eq!(
            read_message(&mut ((MAX_MESSAGE as u32 + 1).to_le_bytes()).as_slice())
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );
    }
}
