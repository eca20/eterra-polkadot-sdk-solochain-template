## ♻️ Updating Runtime After Adding or Modifying a Pallet

When you modify the runtime (adding or changing a pallet, updating config, bumping `spec_version`, etc.), you must rebuild the runtime, regenerate chain specs, purge the old database, re‑insert keys, and restart the node.

Below is the **complete, reliable, copy‑and‑paste sequence**.

---

### ✅ **1. Build the updated runtime + node**

```bash
cargo build -r -p solochain-eterra-runtime -p solochain-eterra-node
```

---

### ✅ **2. Remove old chain specs**

```bash
rm -f chain-specs/testnet.json \
      chain-specs/testnet-plain.json \
      chain-specs/testnet-raw.json
```

---

### ✅ **3. Generate a fresh chain spec (plain)**

```bash
./target/release/solochain-eterra-node build-spec \
  --chain testnet > chain-specs/testnet-plain.json
```

---

### ✅ **4. Generate the raw chain spec**

```bash
./target/release/solochain-eterra-node build-spec \
  --chain chain-specs/testnet-plain.json --raw > chain-specs/testnet-raw.json
```

---

### ✅ **5. Completely purge the testnet DB**

```bash
BASE=/var/lib/eterra-testnet/alice

rm -rf "$BASE"
sudo rm -rf /var/lib/eterra-testnet
sudo mkdir -p /var/lib/eterra-testnet/alice
sudo chown -R "$USER":staff /var/lib/eterra-testnet
```

---

### ✅ **6. Create networking directories**

```bash
BASE=/var/lib/eterra-testnet/alice

mkdir -p "$BASE/chains/eterra_testnet/network"
```

---

### ✅ **7. Generate libp2p networking key**

```bash
./target/release/solochain-eterra-node key generate-node-key \
  --chain chain-specs/testnet-raw.json \
  --file "$BASE/chains/eterra_testnet/network/secret_ed25519"
```

---

### ✅ **8. Insert AURA key for Alice**

```bash
./target/release/solochain-eterra-node key insert \
  --base-path "$BASE" \
  --chain chain-specs/testnet-raw.json \
  --key-type aura \
  --scheme Sr25519 \
  --suri //Alice
```

---

### ✅ **9. Insert GRANDPA key for Alice**

```bash
./target/release/solochain-eterra-node key insert \
  --base-path "$BASE" \
  --chain chain-specs/testnet-raw.json \
  --key-type gran \
  --scheme Ed25519 \
  --suri //Alice
```

---

### ✅ **10. Start the node with the updated runtime**

```bash
BASE=/var/lib/eterra-testnet/alice

./target/release/solochain-eterra-node \
  --chain chain-specs/testnet-raw.json \
  --base-path "$BASE" \
  --validator --alice \
  --force-authoring \
  --port 30333 --rpc-port 9944 \
  --public-addr /ip4/127.0.0.1/tcp/30333 \
  --unsafe-rpc-external --rpc-cors all
```

---

### 🔍 **Verify the runtime version**

```bash
curl -H "Content-Type: application/json" \
  -d '{"id":1,"jsonrpc":"2.0","method":"state_getRuntimeVersion","params":[]}' \
  http://127.0.0.1:9944
```

Expected output should contain:

```
"specName":"solochain-eterra-runtime"
"specVersion":<your new version>
```

---

### 🛠 Troubleshooting

- If runtime version does **not** update:
  - Ensure no old node process is running.
  - Ensure you regenerated the chain specs *after rebuilding the runtime*.
  - Ensure you purged `/var/lib/eterra-testnet` entirely.
  - Verify you are launching the binary from `./target/release`.

---
