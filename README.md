# plenty: shared history for your fish shell

Simple tools in Rust, communicting over SSH in a binary protocol (TLV).

`plenty` is the client, invoked with `plenty <host>`.
`plentys` is the server, invoked with `ssh <host> plentys`.

Sync process:

1. Create `.local/share/plenty` on the server if it doesn't exist.
2. Create `.local/share/plenty/history.db` on the server if it doesn't exist, with the following schema:

```sql
CREATE TABLE history (
  cmd TEXT,
  when INTEGER,
  extra TEXT
);
```

3. Take a lock on `~/local/share/fish` using `flock(LOCK_SH|LOCK_EX)` on the client.
3. Read `~/.local/share/fish/fish_history` on the client.
4. Batch `INSERT INTO history ON CONFLICT DO NOTHING` on the server.
5. Select the full history on the server, write it to `~/.local/share/fish/fish_history` on the client.
6. Release the lock on the client.
