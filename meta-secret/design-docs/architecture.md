# Meta-Secret: Architecture & Design Document

This document provides a detailed description of the Meta-Secret project architecture, design rationale, and best practices for developers working on the system. The project has been developed in the `@core` directory and features two main components:
- **Server-side logic** – processes device events and synchronizes events with connected devices.
- **Client-side logic** – initiates synchronization, maintains local state changes, and communicates with the server via an event-sourced commit log.

The underlying design is distributed and leverages event sourcing as well as a commit log mechanism to ensure reliable state synchronization and data integrity across devices.

---

## 1. Architectural Overview

Meta-Secret is a distributed secret management system built around the following core concepts:

- **Event Sourcing & Commit Log:**  
  The system models all changes as events that are appended to a commit log. Each event encapsulates a specific state change such as secret splits, claims, device logs, and recovery actions. The primary database abstractions are defined in the `@descriptors` module, with the `GenericKvLogEvent` representing rows in the various tables.

- **Distributed Synchronization:**  
  Communication is not strictly client-server in the traditional sense. Instead, devices work in a distributed fashion—synchronizing events using client-side logic in `sync_gateway.rs`, while the server-side logic in `server_app.rs` processes these events and ensures consistency across the network.

- **Cryptography & Secret Management:**  
  A robust cryptographic library (referred to as the *rage crypto library* in parts of the code) underpins the security of the system. All sensitive operations, such as secret splitting and share encryption, are implemented to enforce strong security guarantees:
  - The **split operation** is handled by `MetaDistributor` (located in the secret modules). This component takes a secret, splits it into multiple shares using a distributed algorithm, encrypts each share (using ECIES or other robust schemes), and distributes them to the appropriate devices.
  - The **recovery operation** is managed by the orchestrator (implemented in `orchestrator.rs`), which reads claims and events from the commit log and coordinates the recovery process as necessary.

---

## 2. Detailed Components & Functionality

### 2.1 Server-Side Components

- **`server_app.rs`:**  
  The server application is responsible for synchronizing events with devices. It receives requests (e.g., read requests, claim events) and processes them according to the commit log mechanism. Key tasks include:
  - Processing incoming synchronization requests.
  - Managing the commit log via `GenericKvLogEvent` to store events from the distributed devices.
  - Sending out updated events to devices that have subscribed to changes.
  
  **Example Flow:**
  1. A device sends a synchronization request to the server.
  2. The server processes this using the event sourcing engine and appends new events to the commit log.
  3. Devices receive updates via subsequent synchronization cycles.

- **Data Modeling with Descriptors:**  
  All database tables and records are modeled via the `@descriptors` module. This module defines object descriptors (e.g., vault logs, shared secret events, device logs) and serves as the backbone for the system's data structure.

---

### 2.2 Client-Side Components

- **`sync_gateway.rs`:**  
  The client gateway is responsible for initiating and managing the synchronization process with the server:
  - It periodically checks for new events on the server.
  - It pushes local changes (such as secret splits or device logs) into the commit log on the server.
  - It uses channels and asynchronous task management (with the help of libraries such as `flume` and `async_std`) to ensure non-blocking synchronization.

- **Local State and Data Transfer:**  
  Local events are captured through data transfer channels (for instance, using `MpscDataTransfer`). The client processes state changes, pushes local log events, and then pulls synchronization responses from the server for further processing.

---

### 2.3 Distributed Algorithm & Event Sourcing

The Meta-Secret project is built around a distributed algorithm that leverages event sourcing principles:

- **Commit Log Replication:**  
  Every change made in the system is captured as an event and appended to a commit log. This log not only serves as the record of what happened but is also the source of truth that devices use to update their state consistently.

- **Event-Driven Synchronization Flow:**  
  A typical event sequence occurs as follows:
  - A client initiates an operation (e.g., splitting a secret).
  - The `MetaDistributor` creates a set of share events using the split algorithm. Each share is encrypted using the device's public key.
  - These events are appended to the commit log via `GenericKvLogEvent`.
  - The orchestrator (on the server or recovery client) monitors the commit log and triggers a recovery operation when necessary.

- **Diagrams & Flow Overview:**

  ```ascii
          +-----------------------+
          |  Client (sync_gateway)|
          +-----------+-----------+
                      |
                      v
         +-------------------------------+
         |      Commit Log (EventSourcing)|
         | GenericKvLogEvent & Descriptors|
         +---------------+---------------+
                         |
         +---------------+---------------+
         |        Server (server_app)    |
         +-------------------------------+
  ```

- **Cryptographic Operations:**  
  The project applies advanced cryptographic techniques to secure secret management:
  - **Splitting & Encryption:** The `MetaDistributor` in the secret module splits the input secret into multiple parts and encrypts each share.
  - **Recovery & Re-Encryption:** The orchestrator examines recovery claims and, if needed, re-encrypts shares for secure recovery operations.

---

## 3. Key Modules & Their Roles

- **Secret & Distribution:**
  - **`meta_secret/core/src/secret/mod.rs`:**  
    Provides functions for splitting secrets (`split`, `split2`) and defines the `MetaEncryptor` structure which encrypts each share.
  - **`MetaDistributor`:**  
    Coordinates the distribution of encrypted secret shares. It interacts with persistent logs (`PersistentSharedSecret`) by saving claim events in the commit log.

- **Recovery & Orchestration:**
  - **`orchestrator.rs`:**  
    Manages the logic needed to handle recovery operations (e.g., reassembling shares, sending recovery claims) based on events present in the commit log.

- **Synchronization & Communication:**
  - **`sync_gateway.rs`:**  
    Implements the client-side synchronization logic by sending and receiving events from the server.
  - **`server_app.rs`:**  
    Implements the server-side logic that processes incoming synchronization requests, updates the commit log, and dispatches events accordingly.

- **Database & Descriptors:**
  - **`descriptors/`:**  
    Contains various modules defining object descriptors (credentials, vault, shared secret, etc.) used to model data objects and events.
  - **`GenericKvLogEvent`:**  
    This generic event structure captures the complete state change information for storage in the commit log and is used across different modules to represent tables.

---

## 4. Design Rationale and Best Practices

- **Modularity:**  
  The system is broken down into discrete modules (secret handling, synchronization, orchestration, and data modeling) to facilitate easier maintenance, testing, and independent evolution of components.

- **Event Sourcing:**  
  By using event sourcing, the system ensures that all state transitions are recorded and easily recoverable. This approach supports both current state reconstruction and historical audits.

- **Distributed Synchronization:**  
  The design avoids single-point communication by distributing synchronization responsibilities across clients and the server. This model increases resilience, scalability, and maintainability.

- **Asynchronous & Non-Blocking IO:**  
  All synchronization and processing tasks are implemented using asynchronous Rust (via `async_std` and `flume`) to ensure high throughput and responsiveness in a multi-device environment.

- **Strong Security & Cryptography:**  
  Cryptographic operations are isolated into well-defined methods (such as those in the `MetaDistributor` and `MetaEncryptor`), ensuring that secret management adheres to strict security practices.

- **Robust Testing:**  
  The `meta-tests` directory and associated fixtures play a crucial role in ensuring that every module (from event handling to synchronization) can be independently tested in isolation as well as together.

---

## 5. Developer Guidelines

- **Understanding the Flow:**  
  Developers are encouraged to review the commit log mechanism and how events propagate through the system. Understanding the role of `GenericKvLogEvent` and the various descriptors is key for working on data modeling.

- **Focus on Abstractions:**  
  Abstractions like the synchronization protocol (`SyncProtocol`) and data transfer channels (`MpscDataTransfer`) help separate concerns between network communication and business logic.

- **Modifying Cryptographic Operations:**  
  Any changes to cryptographic operations—whether in secret splitting or share re-encryption—should maintain the integrity of the commit log and be thoroughly tested.

- **Testing and Validation:**  
  Use the fixtures and test specifications in the `meta-tests` module to ensure that modifications do not break the synchronization and event sourcing invariants.

- **Keep Documentation Updated:**  
  Changes in the commit log schema or event descriptors should be reflected in this document to keep consistency across the design and implementation.

---

## 6. Conclusion

The Meta-Secret project leverages modern distributed system design principles, event sourcing, and robust cryptographic operations to deliver a secure and scalable secret management solution. Its dual approach of client-side synchronization via `sync_gateway.rs` and server-side processing via `server_app.rs` ensures that the system remains resilient, secure, and efficient in a distributed environment.

Developers working on Meta-Secret are encouraged to familiarize themselves with the design rationale provided herein and to follow best practices in modularity, cryptography, and event sourcing.

Happy coding! 