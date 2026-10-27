//! Nachrichten über einen Byte-Strom: 4 Byte Länge (little endian), dann die Nachricht (postcard).

use std::io::{self, Read, Write};

use serde::Serialize;
use serde::de::DeserializeOwned;

/// Größte erlaubte Nachricht. Ein Chunk mit allen Samples ist ≈ 240 KB groß.
pub const MAX_MESSAGE: usize = 4 * 1024 * 1024;

pub fn write_message(writer: &mut impl Write, message: &impl Serialize) -> io::Result<()> {
    let bytes = postcard::to_stdvec(message).map_err(io::Error::other)?;
    let length = u32::try_from(bytes.len()).map_err(io::Error::other)?;
    writer.write_all(&length.to_le_bytes())?;
    writer.write_all(&bytes)?;
    writer.flush()
}

/// Liest die nächste Nachricht. `Ok(None)`, wenn die Gegenseite die Verbindung sauber geschlossen hat.
pub fn read_message<T: DeserializeOwned>(reader: &mut impl Read) -> io::Result<Option<T>> {
    let mut length = [0u8; 4];
    match reader.read_exact(&mut length) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error),
    }
    let length = u32::from_le_bytes(length) as usize;
    if length > MAX_MESSAGE {
        return Err(io::Error::new(io::ErrorKind::InvalidData, format!("Nachricht mit {length} Bytes zu groß")));
    }
    let mut bytes = vec![0u8; length];
    reader.read_exact(&mut bytes)?;
    postcard::from_bytes(&bytes).map(Some).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages::{ChunkKind, ClientMessage};

    #[test]
    fn messages_survive_the_round_trip() {
        let mut buffer = Vec::new();
        let sent = [ClientMessage::Hello { version: 1, seed: 42 }, ClientMessage::Chunk { request: 7, position: [-3, 2, 9], kind: ChunkKind::Full }];
        for message in &sent {
            write_message(&mut buffer, message).unwrap();
        }
        let mut reader = buffer.as_slice();
        for message in &sent {
            assert_eq!(read_message::<ClientMessage>(&mut reader).unwrap().as_ref(), Some(message));
        }
        assert_eq!(read_message::<ClientMessage>(&mut reader).unwrap(), None, "Ende nicht erkannt");
    }

    #[test]
    fn oversized_messages_are_rejected() {
        let mut buffer = ((MAX_MESSAGE + 1) as u32).to_le_bytes().to_vec();
        buffer.extend([0u8; 16]);
        assert!(read_message::<ClientMessage>(&mut buffer.as_slice()).is_err());
    }
}
