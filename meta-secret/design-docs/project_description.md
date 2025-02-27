# Meta Secret Core Project Description

## Project Overview

Meta Secret Core is a Rust-based project that implements a decentralized secret management system. It focuses on providing secure and passwordless authentication and secret orchestration through a distributed architecture. The core principles are built around event sourcing, commit logs, and distributed algorithms to ensure data consistency and reliability across multiple devices and users.

This project is divided into two main components:

1.  **Client-Side Logic**:  Handles user interactions, secret management operations (splitting, recovery), and synchronization with the server. This logic resides within the `meta-secret/core/src/node/app` directory and related modules.
2.  **Server-Side Logic**: Manages data synchronization, event persistence, and coordination between different clients. This logic is primarily located in `meta-secret/core/src/node/server` and related modules.

The system is designed to operate in a distributed environment, where clients (devices) synchronize their state with a server, but the core logic and security are not dependent on a centralized server. This architecture enhances resilience and reduces single points of failure.

## Architectural Overview

The Meta Secret Core architecture can be visualized as a distributed system with the following key components: 

graph LR
Client[Client Device] --> SyncGatewayClient(Sync Gateway)
SyncGatewayClient --> EmbeddedSyncProtocolClient(Sync Protocol)
EmbeddedSyncProtocolClient --> ServerApp(Server Application)
ServerApp --> EmbeddedSyncProtocolServer(Sync Protocol)
EmbeddedSyncProtocolServer --> SyncGatewayVD[Virtual Device Sync Gateway]
SyncGatewayVD --> VirtualDevice[Virtual Device]
ServerApp --> KvStore[Key-Value Store (Database)]
VirtualDevice --> KvStore
Client --> KvStore
style Client fill:#f9f,stroke:#333,stroke-width:2px
style VirtualDevice fill:#ccf,stroke:#333,stroke-width:2px
style ServerApp fill:#cfc,stroke:#333,stroke-width:2px
style KvStore fill:#eee,stroke:#333,stroke-width:2px

**Components:**

*   **Client Device**: Represents a user's device (e.g., mobile phone, computer) running the Meta Secret client application.
*   **Virtual Device**: A simulated device within the testing environment, used to represent another participant in the distributed system.
*   **Sync Gateway**:  A crucial component on both the client and virtual device sides responsible for synchronizing data with the server. It uses a defined `SyncProtocol` to communicate.
*   **Sync Protocol**: Defines the communication interface and logic for data synchronization between clients/virtual devices and the server. `EmbeddedSyncProtocol` is used for in-process communication, especially in testing.
*   **Server Application (`ServerApp`)**: The core server-side component that handles synchronization requests, manages data persistence in the `KvStore`, and enforces business logic.
*   **Key-Value Store (`KvStore`)**:  The underlying database that stores all persistent data, including events, user credentials, vaults, and shared secrets.  This is implemented using an event sourcing pattern.

**Data Flow and Synchronization:**

1.  Clients and virtual devices initiate synchronization through their respective `SyncGateways`.
2.  `SyncGateways` use the `SyncProtocol` to send `SyncRequest` messages to the `ServerApp`.
3.  `ServerApp` processes these requests, interacts with the `KvStore` to retrieve or persist data, and returns `DataSyncResponse` messages.
4.  The `SyncProtocol` delivers the responses back to the `SyncGateways`, which then update the client or virtual device's local state.
5.  All data changes are recorded as events in the `KvStore`, forming a commit log that ensures data consistency and auditability.

## Design Rationale and Best Practices

Meta Secret Core is designed with the following principles in mind:

*   **Decentralization**: Avoidance of central points of failure and control. Operations are distributed across clients and servers.
*   **Security**: Strong cryptographic primitives from the `rage` library are used for encryption, signing, and secure communication.
*   **Event Sourcing**:  The system's state is derived from a sequence of events. This provides:
    *   **Auditability**:  A complete history of all changes is recorded.
    *   **Data Consistency**:  Events are the source of truth, ensuring consistency across the distributed system.
    *   **Temporal Queries**:  The ability to reconstruct past states of the system.
*   **Commit Log**: Events are stored in an append-only log, ensuring immutability and ordered processing.
*   **Asynchronous Operations**:  `async`/`await` is extensively used for non-blocking I/O and efficient concurrency, crucial for network operations and database interactions.
*   **Testability**: The architecture is designed to be testable, with in-memory database implementations (`InMemKvLogEventRepo`) and embedded sync protocols for unit and integration tests.
*   **Modularity**: Components are designed to be modular and loosely coupled, facilitating maintenance and future extensions.
*   **Tracing**:  `tracing` library is used for detailed logging and performance analysis, aiding in debugging and monitoring.

**Best Practices for Developers:**

*   **Event-Driven Thinking**: Understand that state changes are driven by events. When implementing new features, consider what events need to be generated and how they will update the system's state.
*   **Descriptor-Based Database Interactions**: Use descriptors (`ObjectDescriptor`, `VaultDescriptor`, `SsWorkflowDescriptor`, etc.) to define and access different object types in the database. This provides type safety and organization.
*   **Immutable Events**: Events (`GenericKvLogEvent`) should be immutable. Once an event is created and persisted, it should not be modified.
*   **Stateless Services**:  Aim for stateless services where possible. State should primarily be managed within the persistent layer (event store) and client-side application state.
*   **Error Handling**: Implement robust error handling using `anyhow::Result` for propagating errors and providing informative error messages.
*   **Asynchronous Programming**: Be proficient in asynchronous Rust programming using `async`/`await` for efficient and responsive applications.
*   **Cryptographic Best Practices**:  Follow secure coding practices when using cryptographic primitives. Consult with security experts for sensitive operations.
*   **Logging and Tracing**: Utilize `tracing` effectively to log important events, errors, and performance metrics. This is crucial for debugging and monitoring distributed systems.

## Server-Side Functionality (`meta-secret/core/src/node/server`)

The server-side logic is primarily responsible for:

*   **Data Synchronization**:  Handling `SyncRequest` messages from clients and virtual devices, and responding with `DataSyncResponse`. This is managed by `ServerApp` and `ServerDataSync`.
*   **Event Persistence**:  Storing all events in the `KvStore`. The server acts as a central point for event aggregation and distribution.
*   **Server-Side Actions**:  Performing actions initiated by the server or in response to client requests, such as vault replication and shared secret replication (`ServerDataSync`).
*   **Request Handling**:  Receiving and routing different types of requests (`ReadSyncRequest`, `WriteSyncRequest`, `VaultRequest`, `SsRequest`, `ServerTailRequest`) within the `ServerApp`.
*   **Server Tail**: Providing clients with the latest event IDs (`ServerTailResponse`) to facilitate efficient synchronization.

**Key Server-Side Components:**

*   **`ServerApp` (`meta-secret/core/src/node/server/server_app.rs`)**: The main server application entry point. It handles incoming `SyncRequest` messages, routes them to appropriate handlers (like `ServerDataSync`), and manages the overall server lifecycle.
*   **`ServerDataSync` (`meta-secret/core/src/node/server/server_data_sync.rs`)**:  Implements the core data synchronization logic. It handles vault replication (`vault_replication`), shared secret replication (`ss_replication`), and processes events to maintain data consistency.
*   **`request.rs` (`meta-secret/core/src/node/server/request.rs`)**: Defines the structure of requests and responses used for communication between clients/virtual devices and the server (e.g., `SyncRequest`, `ReadSyncRequest`, `WriteSyncRequest`, `ServerTailRequest`).
*   **`meta_server.rs` (and `meta_server_serverless` in `meta-secret/meta-server-serverless`)**: Provides a higher-level abstraction for the server, potentially used in serverless environments.

## Client-Side Functionality (`meta-secret/core/src/node/app`)

The client-side logic is responsible for:

*   **User Interface (Conceptual)**: Although not explicitly in `core`, the client-side logic is designed to support a user interface for interacting with the secret management system.
*   **Secret Management**:  Implementing operations like secret splitting (`split` and `split2` in `secret/mod.rs`), encryption, and recovery.
*   **Local Data Persistence**:  Using `PersistentObject` and `KvLogEventRepo` to manage a local event store on the client device.
*   **Synchronization with Server**:  Using `SyncGateway` to synchronize local events with the server and receive updates.
*   **Orchestration (`MetaOrchestrator`)**:  Managing complex workflows like secret recovery and handling vault actions.
*   **Application State Management (`MetaClientService`, `MetaClientStateProvider`)**: Managing the client application's state and handling client requests.

**Key Client-Side Components:**

*   **`SyncGateway` (`meta-secret/core/src/node/app/sync/sync_gateway.rs`)**:  As described earlier, this is the client-side component responsible for synchronization. It polls for updates from the server and pushes local events.
*   **`MetaClientService` (`meta-secret/core/src/node/app/meta_app/meta_client_service.rs`)**:  Handles client-side application logic, manages application state, and processes requests from the user interface (or higher-level application layers).
*   **`MetaOrchestrator` (`meta-secret/core/src/node/app/orchestrator.rs`)**:  Orchestrates complex, multi-step operations like secret recovery and vault management. It reacts to events and triggers actions based on the current state.
*   **`MetaDistributor` (`meta-secret/core/src/secret/mod.rs`)**:  Responsible for splitting secrets into shares and distributing them to vault members during operations like meta-password creation.
*   **`VirtualDevice` (`meta-secret/core/src/node/app/virtual_device.rs`)**:  Used in testing to simulate another device participating in the system. It runs a `SyncGateway` and `MetaOrchestrator` to mimic real client behavior.
*   **`messaging.rs` (`meta-secret/core/src/node/app/meta_app/messaging.rs`)**: Defines messages for client-side application requests and responses (`GenericAppStateRequest`, `GenericAppStateResponse`, `ClusterDistributionRequest`, etc.).

## Distributed Algorithm and Event Sourcing Mechanism

**Distributed Algorithm:**

Meta Secret Core employs a distributed algorithm based on event sourcing and commit logs to achieve consensus and data consistency across devices. The core algorithm can be summarized as follows:

1.  **Event Generation**: Any state change in the system (e.g., user signup, vault creation, secret split, recovery request) is represented as an event (`GenericKvLogEvent`).
2.  **Local Persistence**: Clients and virtual devices persist these events locally in their `KvStore` (using `PersistentObject` and `KvLogEventRepo`).
3.  **Synchronization**:
    *   Clients periodically synchronize with the server through `SyncGateway`.
    *   Synchronization involves:
        *   **Pulling Server Events**: Clients request new events from the server since their last known event ID (`ServerTailRequest`, `ServerTailResponse`).
        *   **Pushing Local Events**: Clients send their locally generated events to the server (`WriteSyncRequest`).
    *   The server acts as an event aggregator and distributor.
4.  **Event Ordering and Consistency**:
    *   Events are ordered within each object's log (e.g., vault log, device log, shared secret log).
    *   The commit log nature of the `KvStore` ensures that events are appended in order and are immutable.
    *   Synchronization mechanisms ensure that all participants eventually receive all events, leading to eventual consistency.
5.  **State Reconstruction**: Clients and the server reconstruct the current state of the system by replaying the sequence of events from their local or server-side event stores.

**Event Sourcing Mechanism:**

*   **`GenericKvLogEvent` (`meta-secret/core/src/node/db/events/generic_log_event.rs`)**:  The fundamental data structure for representing events. It is generic and can encapsulate different types of events within the system.
*   **Descriptors (`meta-secret/core/src/node/db/descriptors`)**:  Used to categorize and identify different types of objects and their associated event logs (e.g., `VaultDescriptor`, `DeviceLogDescriptor`, `SsWorkflowDescriptor`). Descriptors help in querying and organizing events in the `KvStore`.
*   **`KvLogEventRepo` (`meta-secret/core/src/node/db/repo/generic_db.rs`)**:  An interface for interacting with the key-value store and managing event persistence. Implementations like `InMemKvLogEventRepo` are used for testing.
*   **`PersistentObject` (`meta-secret/core/src/node/db/objects/persistent_object.rs`)**:  Provides a higher-level abstraction for interacting with the event store. It simplifies saving, retrieving, and querying events for specific objects based on their descriptors.

## Key Data Structures

*   **`GenericKvLogEvent`**: Represents a single event in the system. It includes:
    *   `key`:  Identifies the object and event type (using descriptors).
    *   `value`:  The actual event data (payload), which varies depending on the event type.
*   **Descriptors (`ObjectDescriptor`, `VaultDescriptor`, `SsWorkflowDescriptor`, etc.)**:  Enumerations and structs that define object types, instances, and their associated event logs. They are used for:
    *   Object identification and categorization.
    *   Database key construction.
    *   Type safety when working with events.
*   **`Vault`**: Represents a user's secure vault for storing secrets. It includes:
    *   Vault metadata (name, members, status).
    *   Configurations (e.g., shared secret configurations).
    *   Associated event logs (vault log, device logs).
*   **`SharedSecret`**: Represents a secret that is split into shares and distributed among vault members. It includes:
    *   Secret metadata (ID, configuration).
    *   Shares (encrypted and distributed to members).
    *   Workflow events related to secret distribution and recovery.
*   **`UserCredentials`**:  Holds user and device-specific credentials, including cryptographic keys and user identifiers.

## Crypto Library (Rage)

Meta Secret Core leverages the `rage` Rust library for cryptographic operations. `rage` provides modern and secure cryptographic primitives, including:

*   **X25519**: For key exchange and establishing secure channels.
*   **ChaCha20Poly1305**: For authenticated encryption (AEAD) of messages and shares.
*   **Ed25519**: For digital signatures and verifying the integrity of data.
*   **Secret Boxes**: For sealed boxes encryption, ensuring confidentiality and authenticity.

The use of `rage` ensures that Meta Secret Core benefits from robust and well-vetted cryptographic implementations, crucial for building a secure secret management system.

## Split and Recovery Operations

**Split Operation (`MetaDistributor`, `secret::split`, `secret::split2`)**:

1.  **Initiation**: A user (or system process) initiates the split operation for a secret (e.g., a meta-password).
2.  **Configuration**: `SharedSecretConfig` defines parameters for the split operation, such as the number of shares and the threshold for recovery.
3.  **Secret Splitting**: The `secret::split` function uses Shamir's Secret Sharing or a similar algorithm (implemented in `SharedSecretEncryption`) to split the secret into multiple shares.
4.  **Encryption**: Each share is encrypted using the recipient's public key (ECIES encryption scheme as seen in `MetaEncryptor::split_and_encrypt`).
5.  **Distribution**: `MetaDistributor` is responsible for:
    *   Creating `SecretDistributionData` objects containing encrypted shares and metadata.
    *   Generating `SsDistributionId` to uniquely identify each distribution.
    *   Persisting distribution events (`SsWorkflowObject::Distribution`) in the `KvStore`.
    *   Potentially sending notifications or initiating synchronization to deliver shares to recipients.
6.  **Claim Creation**: A claim (`SsClaim`) is created to track the distribution and recovery process of the secret.

**Recovery Operation (`MetaOrchestrator::handle_recover`)**:

1.  **Initiation**: A user (device) initiates a recovery process for a shared secret. This might be triggered by a login attempt or a request to access a protected resource.
2.  **Claim Retrieval**: The `MetaOrchestrator` retrieves the relevant `SsClaim` for the secret to be recovered.
3.  **Share Collection**: The device needs to collect enough shares from other vault members (or from its own stored shares if applicable). This might involve:
    *   Requesting shares from other devices (not explicitly shown in the provided code, but implied).
    *   Retrieving locally stored shares.
4.  **Decryption**:  Collected shares are decrypted using the device's private key.
5.  **Secret Reconstruction**:  The decrypted shares are combined using the reverse of the splitting algorithm (implemented in `SharedSecretEncryption`) to reconstruct the original secret.
6.  **Usage**: The recovered secret can then be used for authentication, decryption, or other authorized operations.
7.  **Recovery Workflow Events**:  Events related to the recovery process (e.g., `SsWorkflowObject::Recovery`) are persisted in the `KvStore` to track the workflow and ensure auditability.

This detailed project description should provide developers with a comprehensive understanding of Meta Secret Core's architecture, design principles, and key functionalities. It serves as a valuable resource for onboarding new team members, maintaining the codebase, and extending the system with new features.

