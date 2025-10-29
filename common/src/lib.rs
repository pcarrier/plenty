/// TLV (Type-Length-Value) protocol implementation for plenty
use std::io::{Error, ErrorKind, Read, Result as IoResult, Write};

/// Message types in the TLV protocol
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageType {
    /// History entry: cmd, when, extra
    HistoryEntry = 1,
    /// Request full history from server
    GetHistory = 2,
    /// End of transmission
    End = 3,
    /// Error message
    Error = 4,
}

impl TryFrom<u8> for MessageType {
    type Error = anyhow::Error;

    fn try_from(value: u8) -> Result<Self, <Self as TryFrom<u8>>::Error> {
        match value {
            1 => Ok(MessageType::HistoryEntry),
            2 => Ok(MessageType::GetHistory),
            3 => Ok(MessageType::End),
            4 => Ok(MessageType::Error),
            _ => Err(anyhow::anyhow!("Invalid message type: {}", value)),
        }
    }
}

/// A TLV message
#[derive(Debug, Clone)]
pub struct Message {
    pub msg_type: MessageType,
    pub data: Vec<u8>,
}

impl Message {
    pub fn new(msg_type: MessageType, data: Vec<u8>) -> Self {
        Self { msg_type, data }
    }

    /// Write a TLV message to a writer
    pub fn write_to<W: Write>(&self, writer: &mut W) -> IoResult<()> {
        // Type (1 byte)
        writer.write_all(&[self.msg_type as u8])?;

        // Length (4 bytes, big-endian)
        let len = self.data.len() as u32;
        writer.write_all(&len.to_be_bytes())?;

        // Value
        writer.write_all(&self.data)?;

        writer.flush()
    }

    /// Read a TLV message from a reader
    pub fn read_from<R: Read>(reader: &mut R) -> IoResult<Self> {
        // Read type (1 byte)
        let mut type_buf = [0u8; 1];
        reader.read_exact(&mut type_buf)?;
        let msg_type = MessageType::try_from(type_buf[0])
            .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

        // Read length (4 bytes, big-endian)
        let mut len_buf = [0u8; 4];
        reader.read_exact(&mut len_buf)?;
        let len = u32::from_be_bytes(len_buf) as usize;

        // Read value
        let mut data = vec![0u8; len];
        reader.read_exact(&mut data)?;

        Ok(Message { msg_type, data })
    }
}

/// History entry structure
#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub cmd: String,
    pub when: i64,
    pub extra: String,
}

impl HistoryEntry {
    pub fn new(cmd: String, when: i64, extra: String) -> Self {
        Self { cmd, when, extra }
    }

    /// Encode history entry as TLV message data
    pub fn encode(&self) -> Vec<u8> {
        let mut data = Vec::new();

        // cmd length (4 bytes) + cmd
        let cmd_bytes = self.cmd.as_bytes();
        data.extend_from_slice(&(cmd_bytes.len() as u32).to_be_bytes());
        data.extend_from_slice(cmd_bytes);

        // when (8 bytes)
        data.extend_from_slice(&self.when.to_be_bytes());

        // extra length (4 bytes) + extra
        let extra_bytes = self.extra.as_bytes();
        data.extend_from_slice(&(extra_bytes.len() as u32).to_be_bytes());
        data.extend_from_slice(extra_bytes);

        data
    }

    /// Decode history entry from TLV message data
    pub fn decode(data: &[u8]) -> anyhow::Result<Self> {
        let mut pos = 0;

        // Read cmd
        if data.len() < pos + 4 {
            return Err(anyhow::anyhow!("Invalid data: too short for cmd length"));
        }
        let cmd_len =
            u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        pos += 4;

        if data.len() < pos + cmd_len {
            return Err(anyhow::anyhow!("Invalid data: too short for cmd"));
        }
        let cmd = String::from_utf8(data[pos..pos + cmd_len].to_vec())?;
        pos += cmd_len;

        // Read when
        if data.len() < pos + 8 {
            return Err(anyhow::anyhow!("Invalid data: too short for when"));
        }
        let when = i64::from_be_bytes([
            data[pos],
            data[pos + 1],
            data[pos + 2],
            data[pos + 3],
            data[pos + 4],
            data[pos + 5],
            data[pos + 6],
            data[pos + 7],
        ]);
        pos += 8;

        // Read extra
        if data.len() < pos + 4 {
            return Err(anyhow::anyhow!("Invalid data: too short for extra length"));
        }
        let extra_len =
            u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        pos += 4;

        if data.len() < pos + extra_len {
            return Err(anyhow::anyhow!("Invalid data: too short for extra"));
        }
        let extra = String::from_utf8(data[pos..pos + extra_len].to_vec())?;

        Ok(HistoryEntry { cmd, when, extra })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_history_entry_encode_decode() {
        let entry = HistoryEntry::new("ls -la".to_string(), 1234567890, "paths: /home".to_string());
        let encoded = entry.encode();
        let decoded = HistoryEntry::decode(&encoded).unwrap();

        assert_eq!(entry.cmd, decoded.cmd);
        assert_eq!(entry.when, decoded.when);
        assert_eq!(entry.extra, decoded.extra);
    }

    #[test]
    fn test_message_write_read() {
        let entry = HistoryEntry::new("echo test".to_string(), 9876543210, "".to_string());
        let msg = Message::new(MessageType::HistoryEntry, entry.encode());

        let mut buffer = Vec::new();
        msg.write_to(&mut buffer).unwrap();

        let mut reader = &buffer[..];
        let read_msg = Message::read_from(&mut reader).unwrap();

        assert_eq!(msg.msg_type, read_msg.msg_type);
        assert_eq!(msg.data, read_msg.data);
    }
}
