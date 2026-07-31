# Alpha Backup And Restore

Use the alpha deploy helpers from this directory to take preliminary chain +
IPFS + media snapshots. Nexus V2 final-reset evidence must instead use the
cross-host `scripts/nexus-v2-private-alpha/final_freeze.py` coordinator.

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

This helper does not stop Caddy, the site/indexer/Mongo host, or the arcade
authority and it restarts the chain/media stack. It therefore cannot be used as
the final frozen backup. The final-freeze coordinator keeps all stopped roles
stopped, invokes only SHA-256-pinned component drivers, captures the complete
closed artifact set, and emits same-block gate/inventory evidence. Its chain
host protocol implementation is
`nexus-v2-final-freeze-chain-driver`; the pinned web commit must supply the
matching `site-ingress` and `site-indexer-mongo` roles.

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
