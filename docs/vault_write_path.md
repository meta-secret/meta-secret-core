### Meta Secret

#### Vault "write path"

```mermaid

flowchart TD
    subgraph "meta_secret (vault: write)"
        direction TB

        subgraph clients
            client_a
            client_b
            client_c
        end

        subgraph server
            subgraph server_db
                device_log_a[(device_log_a)]
                device_log_b[(device_log_b)]
                device_log_c[(device_log_c)]

                vault_log[(vault_log)]
                vault[(vault)]
                
                vault_status_a[(vault_status_a)]
                vault_status_b[(vault_status_b)]
                vault_status_c[(vault_status_c)]
            end

            client_a--device_log_a-->server_app_writes

            client_b--device_log_b-->server_app_writes
            client_c--device_log_c-->server_app_writes

            server_app_writes--save-->device_log_a
            server_app_writes--save-->device_log_b
            server_app_writes--save-->device_log_c

            device_log_a--enqueue-->vault_log
            device_log_b--enqueue-->vault_log
            device_log_c--enqueue-->vault_log
            vault_log--create||update-->vault
            
            vault--update_status-->vault_status_a
            vault--update_status-->vault_status_b
            vault--update_status-->vault_status_c
        end
    end
    
