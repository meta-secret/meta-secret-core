# Project Glossary — meta-secret-core

Unified vocabulary for meta-secret-core Rust backend. All communication (AI, code, docs, comments) uses these terms consistently.

**Last updated:** 2026-06-22  
**Maintenance:** Monthly or when architecture changes  
**Scope:** Cryptography, protocols, server logic, and mobile FFI

---

## 1. Cryptography & Key Management

| Term | Definition | Context | Example |
|------|-----------|---------|---------|
| **Master Key** | Per-device root encryption key; derived from user password/passphrase | Key generation | `generate_master_key()` |
| **Device Master Key (DMK)** | Alias for Master Key when stored on specific device | Device storage | iOS Keychain / Android Keystore |
| **Key Share** | Individual cryptographic share in Shamir Secret Sharing (k out of n) | Secret sharing | Each device holds 1 share |
| **Shamir Secret Sharing (SSS)** | Cryptographic scheme: split secret into n shares, recover with k shares (k ≤ n) | Core algorithm | `threshold = k, total_shares = n` |
| **Threshold (k)** | Minimum number of shares needed to recover a secret | SSS parameter | `k = n - 1` (one device can be offline) |
| **Share Pool** | Collection of n key shares distributed among vault members | Vault state | Stored in DB, one per device |
| **Ephemeral Key** | Short-lived encryption key used once, then discarded | Protocol security | Device-to-device communication |
| **Public Key** | Asymmetric crypto: used for encryption, shared openly | Device registration | DSA signing key + Transport key |
| **Private Key** | Asymmetric crypto: kept secret, used for decryption/signing | Device storage | Never transmitted |
| **OpenBox** | Sealed container holding device's public keys: `dsa_pk` + `transport_pk` | Device info | Sent to server during registration |

---

## 2. Secret Sharing & Distribution

| Term | Definition | Context | Example |
|------|-----------|---------|---------|
| **Pass** | A secret value to be encrypted and shared among vault members | Core entity | Password, seed phrase, API key |
| **Pass Distribution** | Process: create shares from secret → send to each device → confirm delivery | Secret lifecycle | `DistributionType::Split` |
| **Pass Recovery** | Process: collect k shares from devices → combine → reveal secret | Secret lifecycle | `DistributionType::Recover` |
| **Claim** | Request object: distribute or recover a pass among vault members | Protocol | `ClaimObject` in API, `Claim` in core |
| **Claim ID** | Unique identifier for a claim | Claim tracking | 32-byte hash |
| **Claim Status** | Enum: `Pending`, `Sent`, `Delivered`, `Accepted`, `Declined` | Claim lifecycle | Per-device status |
| **Distribution Type** | Enum: `Split` (share) or `Recover` (combine shares) | Claim type | Sets claim behavior |
| **Resharing** | Regenerating shares after member leaves vault (k remains same) | Vault ops | New SS setup, same k value |
| **Key Rotation** | Changing all shares after security concern (k may change) | Vault ops | Full regeneration of SSS |

---

## 3. Vault Model & Membership

| Term | Definition | Context | Example |
|------|-----------|---------|---------|
| **Vault** | Encrypted container: collection of passes, members, and shares | Core feature | Owned by initiator device |
| **Vault Name** | Email/identifier uniquely naming the vault | Vault identity | Used as lookup key |
| **Vault Initialization** | Process: device creates vault, generates initial shares, stores locally | First device | Happens on sign-up |
| **Member** | Device with complete vault access and one key share | Vault state | Full participant |
| **Member State** | AppState when user is a vault member | User lifecycle | Can access all secrets |
| **Outsider** | Device requesting to join vault; has no shares yet | Vault state | Pending approval |
| **Outsider State** | AppState when user has requested join but not approved | User lifecycle | Waiting for k-1 devices |
| **Non-Member** | Device with no vault relationship (sign-up hasn't started) | User lifecycle | New email, no vault found |
| **Join Request** | Device data sent by outsider requesting vault membership | Join flow | Contains device info + proof |
| **Join Approval** | Owner/devices approving an outsider's request | Join flow | Incremental: each device approves |
| **Approval Quorum** | Number of approvals needed to admit outsider (configurable, typically k-1) | Join rule | Must match protocol definition |
| **Device Registration** | Adding device to vault: assign device ID, store device info, generate share | Join completion | Final step after quorum reached |
| **Device List** | All devices in vault with their IDs, public keys, and roles | Vault state | Used for target selection in claims |

---

## 4. Device Management

| Term | Definition | Context | Example |
|------|-----------|---------|---------|
| **Device ID** | Unique 32-byte identifier for a device | Device identity | Assigned during registration |
| **Device Data** | Full device record: ID, name, type, public keys, status | Vault state | Stored in core db |
| **Device Type** | Enum: `Android`, `Iphone`, `Tablet`, `Desktop`, `Cli`, `Web` | Device classification | Used for UI icons |
| **Device Offline** | Device unable to respond (network down or app closed) | Claim operation | Doesn't block if k shares available |
| **Device Removal** | Process: remove device from vault, reshare remaining shares | Vault ops | Requires resharing |
| **Primary Device** | First device to join vault; initiator of vault creation | Vault lifecycle | Ownership marker |
| **Secondary Device** | Any device added after vault creation | Vault lifecycle | Via join flow |
| **Active Session** | Current device with valid auth state and socket connection | Runtime | Can participate in claims |

---

## 5. Protocol & Node Communication

| Term | Definition | Context | Example |
|------|-----------|---------|---------|
| **Claim Message** | Protocol message sent over socket: claim announcement + device response | Communication | `SocketAction` variant |
| **Socket Connection** | Persistent WebSocket or transport to server/node | Runtime | Enables real-time updates |
| **Socket Event** | Message from server indicating claim/vault update | Event handling | Triggers UI updates |
| **Node Orchestration** | Server logic coordinating device responses for claims | Server logic | Waits for quorum, confirms delivery |
| **Broadcast** | Sending message to all devices in vault (or subset) | Communication | Announce new claim |
| **Unicast** | Sending message to single device | Communication | Device-specific response |
| **Share Delivery** | Process of transmitting encrypted share to device | Distribution | Share is sent to device's public key |
| **Confirmation** | Device acknowledging receipt and successful decryption of share | Distribution | Required for `Delivered` status |

---

## 6. Data Models & State

| Term | Definition | Context | Example |
|------|-----------|---------|---------|
| **AppState** | Top-level user state: `Local` (no vault) or `Vault` (in vault) | State machine | Result of auth/vault queries |
| **VaultFullInfo** | Sealed enum: `NotExists`, `Outsider`, `Member` — vault membership state | Vault lookup | Determines next UI screen |
| **VaultData** | Persistent vault record: name, members, shares, passes | Storage | Serialized to DB |
| **Share Record** | Stored key share: claim ID + encrypted share blob + device target | Storage | Per-device, one per vault |
| **Pass Record** | Stored secret: ID, name, type, creation metadata | Storage | Encrypted at rest |
| **Claim Record** | Stored claim: ID, type, targets, status per device, created at | Storage | Audit trail |
| **Event Log** | Sequence of vault operations (join, claim, removal) | Audit | For replay/recovery |
| **JSON Model** | Model serialized to/from JSON for FFI boundary | Mobile interface | All FFI args/returns |
| **Error Result** | Variant type: `Ok(T)` or `Err(Error)` — operation outcome | Rust idiomatic | Explicit error handling |

---

## 7. Lifecycle & Operations

| Term | Definition | Context | Example |
|------|-----------|---------|---------|
| **Sign-Up** | Process: new user creates vault on first device | User onboarding | Generates Master Key + shares |
| **Sign-In** | Process: returning user authenticates and loads vault | User auth | Validates Master Key |
| **First Launch** | App startup when no device data exists | Initialization | Prompts for vault name |
| **App Restart** | Resume session: restore vault state from storage | Lifecycle | May find pending claims |
| **Vault Lock** | Master Key cleared from memory; app requires re-auth | Security | Biometric/PIN re-entry needed |
| **Vault Unlock** | Restoring Master Key after auth; decryption available | Security | Opposite of lock |
| **Graceful Shutdown** | Closing socket, saving state, clearing sensitive memory | Exit | Before app termination |
| **Background Mode** | App running without UI focus; still processes socket events | iOS/Android | May receive claim updates |
| **Cold Start** | App launch from fully killed state (vs warm start) | Startup path | Full initialization path |

---

## 8. Server & Persistence

| Term | Definition | Context | Example |
|------|-----------|---------|---------|
| **Server Node** | HTTP server instance handling vault orchestration | Deployment | `meta-server` crate |
| **SQLite DB** | Persistent storage for vault records | Storage | Encrypted at rest optional |
| **DB Transaction** | ACID operation: all changes commit or all rollback | Data consistency | Claim approval atomic |
| **State Resolver** | Component reconciling client request with server state | Protocol logic | Validates against stored records |
| **Indexing** | DB query optimization on vault name or device ID | Performance | Fast device lookup |
| **Replication** | (Future) vault sync across multiple server nodes | Reliability | Not yet implemented |
| **Backup** | Exporting vault state (encrypted or plaintext) | Recovery | User-initiated export |

---

## 9. UniFFI & Mobile Binding

| Term | Definition | Context | Example |
|------|-----------|---------|---------|
| **UniFFI** | Rust FFI abstraction layer generating Kotlin/Swift bindings | Mobile bridge | `uniffi` crate macro |
| **FFI Boundary** | API surface between Rust core and Kotlin/Swift consumers | Contract | Must be stable, backward-compatible |
| **MetaSecretCore** | FFI struct exported to mobile; main entry point | Mobile API | `MetaSecretCoreInterface` in compose |
| **JSON Serialization** | Rust models → JSON → mobile deserialization | Mobile transport | All FFI args/returns are JSON strings |
| **Callback** | Mobile code responding to Rust async completion | Event handling | Called when share received |
| **Error Code** | Integer or string error identifier sent to mobile | Error propagation | Mobile translates to user message |

---

## 10. Security & Cryptography Concepts

| Term | Definition | Context | Example |
|------|-----------|---------|---------|
| **End-to-End Encryption (E2E)** | Encryption from device to device; server cannot decrypt | Threat model | Core security property |
| **Zero-Knowledge** | No secrets shared with server; all computation on devices | Trust model | Server never learns shares |
| **Attack Surface** | Paths where adversary could compromise security | Threat analysis | Device theft, network eavesdrop |
| **Threat Model** | Defined adversary capabilities and protections needed | Security design | See SECURITY.md |
| **Compliance** | Meeting security standards (SOC 2, ISO 27001) | Governance | Future requirement |
| **Audit Trail** | Immutable log of all vault operations | Accountability | Claim approvals + device changes |
| **Non-Repudiation** | Proving who performed an action (crypto signature) | Accountability | Claim approval is signed |

---

## 11. Testing & Quality

| Term | Definition | Context | Example |
|------|-----------|---------|---------|
| **Unit Test** | Testing single function or method in isolation | Quality | Crypto primitives tested |
| **Integration Test** | Testing multiple components together (e.g., claim + store) | Quality | Full flow validation |
| **Property Test** | Generating random inputs and verifying invariants | Quality | SSS recovery always works |
| **Fixture** | Pre-built test data (vault, devices, claims) | Test setup | Reusable across tests |
| **Mock** | Fake object replacing real service for testing | Isolation | Socket mock for offline test |
| **Coverage** | Percentage of code executed by tests | Metrics | Target: >= 80% |

---

## Usage Rules

### When Writing Code:
- **Comments:** Use exact glossary terms (not synonyms)
- **Variables:** Name after glossary term (`vault_name`, not `vname` or `vault_identifier`)
- **Log messages:** "Device joined vault" not "User added to vault"

### When Communicating:
- **Code reviews:** "This claim needs k approvals" not "This request needs majority"
- **Architecture docs:** "SSS with k=n-1 threshold" not "Secret split into n-1 shares"
- **Error messages:** "Device limit exceeded" not "Too many machines"

### Exceptions:
- **Internal variables** in tight loops may abbreviate (`dk` for device_key if clear from context)
- **Acronyms** allowed if expanded at first use ("Device Master Key (DMK)")
- **Historical code** not immediately rewritten, but updated on refactor

---

## Cross-Domain Mapping (compose ↔ core)

| Compose Term | Core Term | Bridge |
|---|---|---|
| Email confirmation | Sign-in validation | Email = vault name lookup |
| Join decision dialog | Join request submission | User approves join attempt |
| Device list screen | Device management query | Core returns device data |
| Secret recovery | Pass recovery flow | Same SSS algorithm |
| Biometric auth | Master Key unlock | Secured in local storage |

---

**Next:** See `.ai/CONSTRAINTS.md` for architectural rules using this vocabulary.
