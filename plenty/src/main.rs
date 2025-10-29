use anyhow::{Context, Result, bail};
use plenty_common::{HistoryEntry, Message, MessageType};
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio, ChildStdin, ChildStdout};
use nix::fcntl::{flock, FlockArg};
use std::os::unix::io::AsRawFd;

fn parse_fish_history(content: &str) -> Result<Vec<HistoryEntry>> {
    let mut entries = Vec::new();
    let mut current_cmd: Option<String> = None;
    let mut current_when: Option<i64> = None;
    let mut current_paths = String::new();

    for line in content.lines() {
        if line.starts_with("- cmd: ") {
            if let (Some(cmd), Some(when)) = (current_cmd.take(), current_when.take()) {
                entries.push(HistoryEntry::new(cmd, when, current_paths.clone()));
                current_paths.clear();
            }
            current_cmd = Some(line[7..].to_string());
        } else if line.starts_with("  when: ") {
            current_when = line[8..].parse().ok();
        } else if line.starts_with("  paths:") {
            current_paths = line.to_string();
        }
    }

    if let (Some(cmd), Some(when)) = (current_cmd, current_when) {
        entries.push(HistoryEntry::new(cmd, when, current_paths));
    }

    Ok(entries)
}

fn format_fish_history(entries: &[HistoryEntry]) -> String {
    let mut output = String::new();
    for entry in entries {
        output.push_str(&format!("- cmd: {}\n", entry.cmd));
        output.push_str(&format!("  when: {}\n", entry.when));
        if !entry.extra.is_empty() {
            output.push_str(&format!("{}\n", entry.extra));
        }
    }
    output
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <host>", args[0]);
        std::process::exit(1);
    }

    let host = &args[1];

    let fish_dir = if let Ok(xdg_data_home) = std::env::var("XDG_DATA_HOME") {
        PathBuf::from(xdg_data_home).join("fish")
    } else {
        let home = std::env::var("HOME").context("HOME environment variable not set")?;
        PathBuf::from(&home).join(".local/share/fish")
    };
    let history_path = fish_dir.join("fish_history");

    std::fs::create_dir_all(&fish_dir)
        .context("Failed to create fish directory")?;

    let lock_dir = std::fs::File::open(&fish_dir)
        .context("Failed to open fish directory for locking")?;

    eprintln!("Acquiring lock on fish directory…");
    flock(lock_dir.as_raw_fd(), FlockArg::LockExclusive)
        .context("Failed to acquire lock on fish directory")?;

    let history_file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(&history_path)
        .context("Failed to open fish_history file")?;

    let result = sync_with_server(host, &history_path, &history_file);

    flock(lock_dir.as_raw_fd(), FlockArg::Unlock)
        .context("Failed to release lock on fish directory")?;

    result
}

fn sync_with_server(host: &str, history_path: &PathBuf, history_file: &File) -> Result<()> {
    eprintln!("Reading local fish history…");
    let mut content = String::new();
    let mut reader = BufReader::new(history_file);
    reader.read_to_string(&mut content)
        .context("Failed to read fish_history")?;

    let local_entries = parse_fish_history(&content)
        .context("Failed to parse fish_history")?;

    eprintln!("Found {} local history entries", local_entries.len());

    eprintln!("Connecting to {}…", host);
    let mut ssh_process = Command::new("ssh")
        .arg(host)
        .arg("plentys")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .context("Failed to start ssh process")?;

    let ssh_stdin = ssh_process.stdin.take()
        .context("Failed to get ssh stdin")?;
    let ssh_stdout = ssh_process.stdout.take()
        .context("Failed to get ssh stdout")?;

    let mut writer = BufWriter::new(ssh_stdin);
    let mut reader = BufReader::new(ssh_stdout);

    eprintln!("Sending local history to server…");
    for entry in &local_entries {
        let msg = Message::new(MessageType::HistoryEntry, entry.encode());
        msg.write_to(&mut writer)
            .context("Failed to send history entry to server")?;
    }

    eprintln!("Requesting full history from server…");
    let get_history_msg = Message::new(MessageType::GetHistory, Vec::new());
    get_history_msg.write_to(&mut writer)
        .context("Failed to send GetHistory request")?;

    eprintln!("Receiving history from server…");
    let mut server_entries = Vec::new();

    loop {
        let msg = Message::read_from(&mut reader)
            .context("Failed to read message from server")?;

        match msg.msg_type {
            MessageType::HistoryEntry => {
                let entry = HistoryEntry::decode(&msg.data)
                    .context("Failed to decode history entry from server")?;
                server_entries.push(entry);
            }
            MessageType::End => {
                break;
            }
            MessageType::Error => {
                let error_msg = String::from_utf8_lossy(&msg.data);
                bail!("Server error: {}", error_msg);
            }
            _ => {
                bail!("Unexpected message type from server");
            }
        }
    }

    eprintln!("Received {} history entries from server", server_entries.len());

    let end_msg = Message::new(MessageType::End, Vec::new());
    end_msg.write_to(&mut writer)
        .context("Failed to send End message")?;

    drop(writer);

    let status = ssh_process.wait()
        .context("Failed to wait for ssh process")?;

    if !status.success() {
        bail!("SSH process exited with status: {}", status);
    }

    eprintln!("Writing updated history to local file…");
    let new_content = format_fish_history(&server_entries);

    let mut file = OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(history_path)
        .context("Failed to open fish_history for writing")?;

    file.write_all(new_content.as_bytes())
        .context("Failed to write fish_history")?;

    eprintln!("Sync complete!");

    Ok(())
}
