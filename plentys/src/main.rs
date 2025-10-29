use anyhow::{Context, Result};
use plenty_common::{HistoryEntry, Message, MessageType};
use rusqlite::{Connection, params};
use std::io::{stdin, stdout, BufReader, BufWriter};
use std::path::PathBuf;

fn main() -> Result<()> {
    // Set up database path - respect XDG_DATA_HOME
    let data_dir = if let Ok(xdg_data_home) = std::env::var("XDG_DATA_HOME") {
        PathBuf::from(xdg_data_home).join("plenty")
    } else {
        let home = std::env::var("HOME").context("HOME environment variable not set")?;
        PathBuf::from(home).join(".local/share/plenty")
    };

    // Create directory if it doesn't exist
    std::fs::create_dir_all(&data_dir)
        .context("Failed to create plenty directory")?;

    let db_path = data_dir.join("history.db");

    // Open/create database
    let conn = Connection::open(&db_path)
        .context("Failed to open database")?;

    // Create table if it doesn't exist
    conn.execute(
        "CREATE TABLE IF NOT EXISTS history (
            cmd TEXT,
            \"when\" INTEGER,
            extra TEXT
        )",
        [],
    ).context("Failed to create history table")?;

    conn.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_history_unique
         ON history(cmd, \"when\", extra)",
        [],
    ).context("Failed to create unique index")?;

    let stdin = stdin();
    let stdout = stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut writer = BufWriter::new(stdout.lock());

    // Process incoming messages
    loop {
        let msg = match Message::read_from(&mut reader) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                // Client closed connection
                break;
            }
            Err(e) => {
                eprintln!("Error reading message: {}", e);
                let error_msg = Message::new(
                    MessageType::Error,
                    format!("Error reading message: {}", e).into_bytes(),
                );
                let _ = error_msg.write_to(&mut writer);
                break;
            }
        };

        match msg.msg_type {
            MessageType::HistoryEntry => {
                // Decode and insert history entry
                match HistoryEntry::decode(&msg.data) {
                    Ok(entry) => {
                        if let Err(e) = conn.execute(
                            "INSERT OR IGNORE INTO history (cmd, \"when\", extra) VALUES (?1, ?2, ?3)",
                            params![entry.cmd, entry.when, entry.extra],
                        ) {
                            eprintln!("Error inserting history entry: {}", e);
                            let error_msg = Message::new(
                                MessageType::Error,
                                format!("Error inserting history: {}", e).into_bytes(),
                            );
                            let _ = error_msg.write_to(&mut writer);
                        }
                    }
                    Err(e) => {
                        eprintln!("Error decoding history entry: {}", e);
                        let error_msg = Message::new(
                            MessageType::Error,
                            format!("Error decoding history entry: {}", e).into_bytes(),
                        );
                        let _ = error_msg.write_to(&mut writer);
                    }
                }
            }
            MessageType::GetHistory => {
                // Send all history back to client
                let mut stmt = conn.prepare(
                    "SELECT cmd, \"when\", extra FROM history ORDER BY \"when\" ASC"
                ).context("Failed to prepare select statement")?;

                let entries = stmt.query_map([], |row| {
                    Ok(HistoryEntry::new(
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                    ))
                }).context("Failed to query history")?;

                for entry_result in entries {
                    match entry_result {
                        Ok(entry) => {
                            let msg = Message::new(MessageType::HistoryEntry, entry.encode());
                            msg.write_to(&mut writer)
                                .context("Failed to write history entry")?;
                        }
                        Err(e) => {
                            eprintln!("Error reading history entry: {}", e);
                        }
                    }
                }

                // Send end marker
                let end_msg = Message::new(MessageType::End, Vec::new());
                end_msg.write_to(&mut writer)
                    .context("Failed to write end marker")?;
            }
            MessageType::End => {
                // Client signaling end of transmission
                break;
            }
            MessageType::Error => {
                eprintln!("Received error from client: {}",
                    String::from_utf8_lossy(&msg.data));
                break;
            }
        }
    }

    Ok(())
}
