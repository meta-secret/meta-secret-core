## meta-secret-core

#### "Join Cluster" operation, sequence diagram
```mermaid

sequenceDiagram

    box Server 
        participant app_server
        participant db_server
    end

    box DeviceA 
        participant app_a
        participant read_db_a
        participant db_sync_a
    end

    box DeviceA_Db
        participant db_a
        participant Mempool_a
        participant GlobalIndex_a
        participant MetaVault_a
        participant UserCreds_a
        participant Vault_a
        participant DbTail_a
    end

    box DeviceB 
        participant app_b
        participant db_sync_b
        participant db_b
    end

    box DeviceC 
        participant app_c
        participant db_sync_c
        participant db_c
    end

app_a ->>+ db_a: get_local_vault(meta_vault, user_creds)
db_a -->> MetaVault_a: get meta_vault
db_a -->> UserCreds_a: get creds
db_a -->>- app_a: provide meta vault and creds

app_a ->>+ db_a: get_events(vault, global_index)
db_a -->> Vault_a: get vault
db_a -->> GlobalIndex_a: get global index
db_a -->>- app_a: provide vault and global index

app_a ->> read_db_a: apply db events to read_db

app_a ->> read_db_a: find vault in global index
Note right of app_a: We can join cluster <br/> if vault exists

app_a ->> db_a: mempool(join_event)
db_a -->> Mempool_a: save join_cluster request

db_sync_a ->> db_a: get(db_tail)
db_a -->> DbTail_a: get current db tail
db_a -->> db_sync_a: provide current tail

db_sync_a ->> db_a: find vault events to sync
db_a -->> Vault_a: find new events after db_tail vault_id
db_sync_a ->> app_server: send new vault events

db_sync_a ->> db_a: find memepool events to sync
db_a -->> Mempool_a: find new events
db_sync_a ->> app_server: send new mempool events

app_server -->> app_server: handle mempool requests
app_server ->> db_server: save vault_join_request
Note right of app_server: (made from join_cluster request)
app_server ->> db_server: save vault_update event

db_sync_a ->> db_a: update db_tail (vault, global_index)

db_sync_a ->>+ app_server: sync(db_tail)
app_server -->> db_server: get latest event later than the sync request
app_server -->>- db_sync_a: provide new events

db_sync_a ->> db_a: save new events
app_a ->> db_a: get new events
app_a ->> read_db_a: apply new events to read_db

app_a ->> read_db_a: get vault
app_a ->> app_a: show vault to the user
```

## Fresh JOIN capacity preflight

Before producing a fresh NonMember or Declined JOIN, Core reads the current own status after its existing synchronization and sends `ReadSyncRequest::JoinAdmission(UserData)` through `/meta_request`. The server processes this read under its existing request ordering, before lazy initialization or maintenance. It validates the canonical Vault and exact candidate context without returning the Vault, member list, keys or history to the outsider.

The response is `DataSyncResponse::Empty` when capacity currently permits a request. Refusal is an ordinary `DataSyncResponse::Error` with only `Vault supports at most 3 devices` or `Join admission unavailable`; internal repository errors are not returned. Only Empty permits Core to proceed to the original local JOIN append. Unsupported servers, network errors and unexpected response variants fail closed without that new append. Deploy the compatible server before clients using this additive read; the mobile JSON/UniFFI API is unchanged.

This check does not reserve a slot, grant membership or replace another member's approval. The server validates actual admission again if topology changes after the read. Existing Pending, Member and new-Vault creation flows keep their original behavior. Previously queued or possibly sent requests are not canceled, removed or repaired by preflight. A capacity error is not permission to clear an unknown or accepted registration.

### Verification checkpoint

The real Mobile manager/service queue and HTTP ServerApp regression passes for a fresh fourth candidate: capacity rejection leaves its JOIN outbox unchanged and subsequent ordinary get_state remains usable. The two-member control produces exactly one Pending JOIN and still requires member approval. These Rust integration checks do not establish native UI Back/reset or original Test21 success; those remain separate validation gates.
