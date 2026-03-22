# Alpha Backup And Restore

Use the alpha deploy helpers from this directory to snapshot and restore the current chain + IPFS + media runtime state on the 2010 mini.

## Backup

From the local deploy machine:

```bash
./deploy/alpha/macmini2010/backup-alpha-state.sh
```

Or with a custom backup folder name:

```bash
./deploy/alpha/macmini2010/backup-alpha-state.sh alpha-dry-run-01
```

The script:

- stops the alpha node and media/IPFS services briefly
- snapshots the node base path
- snapshots the IPFS docker volumes
- copies the current `node.env` and `media.env`
- restarts the services
- downloads the backup into `deploy/alpha/macmini2010/.artifacts/backups/<name>/`

## Restore

Run restores only on a non-production dry run unless you intentionally want to roll the alpha environment back.

```bash
./deploy/alpha/macmini2010/restore-alpha-state.sh \
  ./deploy/alpha/macmini2010/.artifacts/backups/alpha-dry-run-01
```

The restore script:

- stops the alpha node and media/IPFS services
- clears the current node/IPFS data
- restores the archived node data, IPFS volumes, and runtime env files
- starts the alpha services again

After restore, run:

```bash
./deploy/alpha/macmini2010/status.sh
./deploy/alpha/macmini2014/status.sh
```
