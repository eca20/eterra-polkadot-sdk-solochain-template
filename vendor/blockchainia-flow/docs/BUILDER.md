# Visual builder

The builder supports local template selection, machine/state/action editing,
conditions, effects, validation diagnostics, warnings, JSON import/export,
deterministic SCALE compilation, and unsigned publish-plan preparation.

It intentionally has:

- no seed phrase, private-key, keystore, browser-wallet, or signing API;
- no RPC submission code;
- no optimistic claim that local validation equals runtime acceptance.

The publish plan contains ordered call names and arguments for an external
wallet integration:

1. `create_game` when the namespace does not exist;
2. `upload_version_chunk` for every exact byte chunk;
3. `finalize_version` with the exact Blake2-256 manifest hash;
4. optional `activate_version`, always disabled by default.

Run it locally after `npm run build` by serving `apps/builder/dist` with any
static file server. The Blockchainia website contains a separate private-alpha
preview, not a production publisher.

