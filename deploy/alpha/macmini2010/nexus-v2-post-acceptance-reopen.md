# Nexus V2 restricted post-acceptance reopen

Status: implemented and covered by offline tests. Creating or testing these
files does not contact either private-Alpha host. The scripts do not authorize
public production, paid features, package publication, or chain-state rollback.

## Boundary

The Phase-1 deployment deliberately binds the six protected services to
loopback and closes their UFW rules. That binding remains unchanged. After the
acceptance boundary, signed Unity proof, proof-policy deactivation, and final
seal, this driver may add a transport layer from the site host to four services:

| Service | Chain-host listener | Backend | Allowed source |
| --- | --- | --- | --- |
| chain RPC | `CHAIN_LAN_IP:9944` | `127.0.0.1:9944` | exact site-host IP |
| media | `CHAIN_LAN_IP:4000` | `127.0.0.1:4000` | exact site-host IP |
| IPFS gateway | `CHAIN_LAN_IP:8080` | `127.0.0.1:8080` | exact site-host IP |
| arcade authority | `CHAIN_LAN_IP:8787` | `127.0.0.1:8787` | exact site-host IP |

Chain P2P `30333` and the IPFS API `5001` are never proxied and retain no UFW
allow rule. `systemd-socket-proxyd` owns the four LAN listeners. Each UFW rule
names both the exact source host and exact destination host; a LAN CIDR is not
accepted. A dedicated early-priority `inet` nftables guard independently drops
all IPv4 and IPv6 traffic to all six protected ports except the four exact
site-host read paths. Extra UFW `ALLOW` or `LIMIT` rules are rejected. Docker
does not publish any of these LAN listeners.

The site component changes only the active Caddyfile. `open` installs the exact
normal Caddyfile from the final-lock-pinned web commit. `close` restores the
exact Phase-1 read-only Caddyfile. Site and indexer containers remain published
only on `127.0.0.1:3000` and `127.0.0.1:8787`; Caddy remains the only public
HTTP/HTTPS boundary.

## Authority chain

`nexus-v2-post-acceptance-reopen.py` requires all of the following before any
driver can resolve credentials or contact a host:

1. The canonical, unexpired reopen plan and its externally supplied SHA-256.
2. The final `nexus-v2-private-alpha-release-lock`, revalidated through
   `release_lock.py`. This rechecks all nine clean source repositories, selected
   chain/site environments, candidates, runtime, metadata, acceptance receipt,
   read-model binding, and disabled production flags.
3. The exact acceptance-boundary receipt pinned by that lock. It must record
   `keep-v2`, a passing Phase-1 smoke, and permanent retirement of automatic
   state restore.
4. The canonical
   `eterra.nexus-v2-runtime-seeder-phase2-final-seal.v1`. Both a local closed
   contract and the official validator from the final-lock-pinned web source
   verify the seal. The temporary proof policy must be inactive, AlphaAccess
   must be enforced, the four-registration authority manifest must be
   non-fixture, and paid entry, wagering, transfers, marketplace, valued
   rewards, permanent asset loss, and public production must remain disabled.
5. The exact chain and site source commits, both Caddyfiles, component driver,
   and host helpers. All bytes are SHA-256 pinned. Driver/helper paths must be
   the canonical paths inside the final-lock-pinned chain source.
6. Two deliberately separate deployment identities: chain `releaseId` and site
   `siteReleaseVersion`. The latter is derived from `RELEASE_VERSION` in the
   final-lock-pinned site environment and must start with `v`; it is never
   conflated with the chain release ID.
7. One known immutable media path/body hash and one known immutable IPFS
   path/body hash for end-to-end read verification.
8. The exact Phase-2 bootstrap prerequisite and final authority manifest. The
   coordinator derives a closed current-state contract from them: spec 106,
   runtime code and metadata hashes, one exact enforced AlphaAccess grant,
   four active Training authority epochs, one zero-reward proof policy, and
   that policy's post-proof inactive state.
9. A fresh, canonical, read-only site deployment identity captured after the
   Phase-2 final seal and while Phase 1 remains closed. It pins the Compose
   bytes, immutable image IDs and references for site/indexer/Mongo/Caddy,
   every Docker publication, and normalized safe FPS and Legends
   status/configuration facts. The earlier Phase-1 deployment receipt is a
   separate artifact and cannot satisfy this reopen prerequisite.
10. The exact dedicated `known_hosts` and canonical SSH host-pin manifest that
    are common artifacts of both the pre-cutover replacement lock and final
    release lock. Both selected deployment environments name those exact paths
    and hashes. Plan capture verifies them with
    `capture_ssh_host_pins.py` and copies them, plus that validator, into the
    immutable emergency-closure bundle. Protected transports ignore ambient
    SSH configuration, reject `SSH_OPTS`, disable every proxy/DNS/update trust
    path, and treat any host-authenticity prompt as fatal.

The plan lifetime is at most 24 hours. Open, verify, and commit revalidate the
complete authority chain. Plan capture also creates a sibling, root-readable
emergency-closure bundle with its own driver, helpers, deployment libraries,
and both Caddyfiles. `close` validates only the immutable plan and that narrow
bundle; expiry, ordinary source drift, missing final artifacts, or marker drift
cannot revoke closure authority. Marker anomalies are archived and healed.
Plan and evidence outputs remain outside every final-lock-pinned repository.

## Offline plan capture

Run from the clean, final-lock-pinned chain source. Paths shown here are
placeholders; all paths must be absolute regular non-symlink files.

```bash
./deploy/alpha/macmini2010/nexus-v2-post-acceptance-reopen.py capture-plan \
  --operation-id nexus-v2-reopen-YYYYMMDDTHHMMSSZ \
  --release-id nexus-v2-private-alpha-RELEASE \
  --source-commit CHAIN_COMMIT \
  --site-source-commit WEB_COMMIT \
  --genesis-hash 0xGENESIS \
  --final-release-lock /secure/evidence/final-release-lock.json \
  --acceptance-boundary-receipt /secure/evidence/acceptance-boundary-receipt.json \
  --phase2-final-seal /secure/evidence/nexus-v2-runtime-seeder-phase2-final-seal.json \
  --phase2-bootstrap-prerequisite /secure/evidence/nexus-v2-runtime-seeder-bootstrap-prerequisite.json \
  --authority-manifest /secure/evidence/eterra-authority-registration-manifest-v1.json \
  --site-deployment-identity /secure/evidence/site-post-deploy-identity.json \
  --site-deployment-candidate-manifest /secure/evidence/site-candidate-manifest.json \
  --site-phase1-post-deploy-identity /secure/evidence/site-phase1-post-deploy-identity.json \
  --selected-deployment-environment /secure/env/macmini2010.env \
  --selected-site-deployment-environment /secure/env/macmini2014.env \
  --ssh-known-hosts /secure/evidence/ssh/nexus-v2-alpha.known_hosts \
  --ssh-host-pin-manifest /secure/evidence/ssh/nexus-v2-alpha.known_hosts.json \
  --normal-caddyfile /ABS/WEB/tcg/deploy/alpha/macmini2014/nexus-v2-restricted-alpha.Caddyfile \
  --phase1-caddyfile /ABS/WEB/tcg/deploy/alpha/macmini2014/nexus-v2-phase1-readonly.Caddyfile \
  --component-driver "$PWD/deploy/alpha/macmini2010/nexus-v2-post-acceptance-reopen-component-driver" \
  --chain-helper "$PWD/deploy/alpha/macmini2010/nexus-v2-post-acceptance-reopen-host-action.sh" \
  --site-helper "$PWD/deploy/alpha/macmini2010/nexus-v2-post-acceptance-reopen-site-action.sh" \
  --chain-lan-ip 192.168.1.159 \
  --site-lan-ip 192.168.1.218 \
  --public-hostname pocket.eterra.online \
  --media-smoke-path /nft/IMMUTABLE_SMOKE_OBJECT \
  --media-smoke-sha256 MEDIA_BODY_SHA256 \
  --ipfs-smoke-path /ipfs/IMMUTABLE_CID \
  --ipfs-smoke-sha256 IPFS_BODY_SHA256 \
  --output /secure/evidence/post-acceptance-reopen-plan.json
```

Capture and validation are offline. Verify the plan again without credential
resolution or host contact:

```bash
./deploy/alpha/macmini2010/nexus-v2-post-acceptance-reopen.py validate \
  --plan /secure/evidence/post-acceptance-reopen-plan.json \
  --expected-sha256 PLAN_SHA256
```

## Protected execution

The coordinator contains no credential transport. Its component adapter loads
the existing, final-lock-selected host environments through their existing
deployment libraries. Protected execution requires both explicit values:

```bash
export NEXUS_V2_POST_ACCEPTANCE_REOPEN_BACKEND=protected-alpha
export NEXUS_V2_POST_ACCEPTANCE_REOPEN_CONFIRMATION=PRIVATE_ALPHA_RESTRICTED_REOPEN

./deploy/alpha/macmini2010/nexus-v2-post-acceptance-reopen.py execute \
  --plan /secure/evidence/post-acceptance-reopen-plan.json \
  --expected-sha256 PLAN_SHA256 \
  --evidence-dir /secure/evidence/post-acceptance-reopen-execute
```

After FPS promotion, the coordinator verifies the live deployment and writes
`fps-adoption-seal.json` into that evidence directory. The seal pins the plan,
final release lock, Unity FPS candidate, selected deployment environment,
promotion receipt, and verification result. Site open, marker, result,
preparation, commit, and every later revalidation all require those exact seal
bytes. Preserve that file with the execution evidence.

An active-boundary verification must reuse the original immutable seal; it
does not rotate or recapture authority. The fresh FPS verification must resolve
to the same pinned deployment receipt before the site helper is invoked:

```bash
./deploy/alpha/macmini2010/nexus-v2-post-acceptance-reopen.py verify \
  --plan /secure/evidence/post-acceptance-reopen-plan.json \
  --expected-sha256 PLAN_SHA256 \
  --fps-adoption-seal /secure/evidence/post-acceptance-reopen-execute/fps-adoption-seal.json \
  --fps-adoption-seal-sha256 FPS_ADOPTION_SEAL_SHA256 \
  --evidence-dir /secure/evidence/post-acceptance-reopen-verify
```

The sequence is fixed:

1. offline chain component preflight;
2. offline site component preflight;
3. read-only protected chain-host preflight;
4. read-only protected site-host preflight;
5. chain-host proxy/UFW open;
6. final Caddyfile activation;
7. chain transport verification; and
8. site/Caddy/read-path verification;
9. site commit preparation and immutable site-prepare result capture while its
   watchdog remains armed;
10. site commit, which accepts that exact site-prepare result before disarming
    public Caddy's watchdog; and
11. final chain commit, which accepts the exact final site-commit result and
    disarms the source-restricted, asset-bearing chain watchdog last.

Before Caddy becomes writable, verification rereads one finalized block and
checks the current genesis, runtime code, metadata, AlphaAccess, storage
cardinalities, authority epochs/windows/config hashes, proof-policy hash,
zero budget, inactive activation, and safe FPS/Legends status. It also covers
local loopback backends, exact LAN proxy listeners, the UFW and nftables guards,
absent `30333`/`5001` exposure, all four site container images/publications,
media and IPFS content hashes, active host/container Caddyfile hashes, upstream
and public reads, and rejected mutation methods.

Both hosts arm independent five-minute fail-closed watchdogs before first
exposure. `EXIT`, `HUP`, `INT`, and `TERM` handlers close locally; the watchdogs
cover coordinator loss and untrappable process death. The source-restricted
chain watchdog is always disarmed last. If coordination is lost between the two
final commits, that watchdog removes every path from public Caddy to the
asset-bearing chain/authority services. On any reported failure, including a
later `verify` failure, the emergency driver first removes the asset-bearing
chain-host transport and then restores Phase-1 Caddy (or stops Caddy if reload
cannot be proven). It removes all proxies, every protected-port UFW permit, and
the dedicated nftables guard. It never invokes backup restore,
changes runtime state, or silently restores chain data after acceptance.

Reverify an active boundary with the original adoption seal, or close it
explicitly with `close`, using a new evidence directory each time. Completed remote actions have
immutable, plan-bound markers and are idempotent. A closed operation cannot be
reopened; capture a new short-lived operation instead.
