# Security policy

Blockchainia Flow `0.1.0-alpha.1` is private-alpha software.

- Do not use it to custody keys or sign transactions.
- Do not use generic Flow variables as the source of truth for inventory,
  balances, randomness, competitive eligibility, or economic rewards.
- Keep runtime bounds enabled and treat the runtime validator as authoritative.
- Treat attested events as untrusted until the configured runtime authority
  provider accepts the exact game, version, event, and sequence.
- Review every prepared transaction in a wallet or operator tool before signing.

Report suspected vulnerabilities privately to `dev@blockchainia.us`. Do not
publish exploit details before coordinated remediation.

