# Alpha Access

AlphaAccess is the canonical V1 whitelist for Alpha. The external Polkadot Hub TestNet pass contract proves purchase intent, but Alpha runtime storage decides whether an account can use gated Alpha app calls.

## V1 flow

1. User purchases an Alpha Access Pass on Polkadot Hub TestNet.
2. The contract emits `AccessPurchased`.
3. The trusted-but-auditable relayer verifies the event, chain ID, contract address, replay key, binding hash, Alpha account signature, and binding expiry.
4. The relayer submits `alphaAccess.grant_access` to Alpha as an authorized manager.
5. AlphaAccess stores the whitelist grant and processed source event.
6. Alpha app pallets and the website check AlphaAccess, not NFT ownership alone.

The relayer manager key should only have AlphaAccess manager authority. Root/sudo or governance manages allowed sources and manager assignment.

## Why XCM is not in V1

V1 intentionally uses a relayer/indexer because Alpha is currently treated as a solochain. XCM is the preferred future path if Alpha becomes a parachain connected to the same Polkadot test network as Polkadot Hub. The AlphaAccess pallet is designed to be XCM-ready by storing generic source metadata, allowed sources, replay-protected source IDs, and source kinds. This lets the project ship a working alpha now without blocking on cross-chain transport, HRMP/XCMP setup, XCM fee/weight handling, or parachain-specific runtime configuration.

V1 does not require XCM precompile calls, XCM receive logic, HRMP/XCMP setup, parachain registration, sovereign account funding, XCM fee purchasing, XCM weight calculation, or bridge/light-client verification.

## Future XCM migration

When Alpha is deployed as a parachain, the AccessPass contract can be extended to call the Polkadot Hub XCM precompile after purchase. The XCM message would carry purchaseId, alphaAccountId, tokenId, source contract, and expiresAtUnix to Alpha. AlphaAccess would then accept the grant only from the expected XCM origin and known contract source. The relayer may remain as a fallback or be retired once XCM reliability is proven.

The V1 storage model already distinguishes `ContractEventRelayer`, `XcmMessage`, and `ManualAdmin` source kinds so relayer grants and future XCM-originated grants can coexist.
