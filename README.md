# plenty: fish shells unified history

## Usage

To unify your fish shell history:

- Install `plentys` on the host of your choice.
- Install `plenty` on your machines.
- Run `plenty <host>` periodically on your machines.

## Design

Simple tools in Rust, communicating over SSH in a binary protocol (TLV).

`plenty` is the client, invoked with `plenty <host>`.
`plentys` is the server, invoked by the client through `ssh <host> plentys`.

### Sync process

1. Create `.local/share/plenty` on the server if it doesn't exist.
2. Create `.local/share/plenty/history.db` on the server if it doesn't exist, with the following schema:

```sql
"CREATE TABLE IF NOT EXISTS history (
  cmd TEXT,
  "when" INTEGER,
  extra TEXT
)

CREATE UNIQUE INDEX IF NOT EXISTS idx_history_unique
ON history("when", cmd, extra)
```

3. Take a lock on `~/local/share/fish` using `flock(LOCK_SH|LOCK_EX)` on the client.
3. Read `~/.local/share/fish/fish_history` on the client.
4. `INSERT OR IGNORE INTO history` on the server.
5. Select the full history on the server `ORDER BY "when"`, send it to the client.
6. Write it to `~/.local/share/fish/fish_history` on the client.
7. Release the lock on the client.
