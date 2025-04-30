# Meta Secret CLI Example

This directory contains example workflows showing how to use the Meta Secret CLI for secret sharing and recovery.

## Overview

The example demonstrates a complete workflow:
1. Building the CLI application
2. Initializing devices and users
3. Creating a vault with multiple members
4. Splitting a secret across devices
5. Recovering the secret when needed

## Running with Make

You can run the entire workflow using Make:

```bash
# Run the full workflow
make run_all

# Run specific steps
make build
make device
make user
# ... and so on

# Clean the environment
make clean
```

## Running with Task

An alternative to Make is using [Task](https://taskfile.dev/), a modern task runner:

1. Install Task:
   ```bash
   # Mac/Linux
   curl -sL https://taskfile.dev/install.sh | sh
   
   # Or with brew on macOS
   brew install go-task/tap/go-task
   
   # For other methods, see https://taskfile.dev/installation/
   ```

2. Run the workflow:
   ```bash
   # Run the full workflow
   task run-all
   
   # See available tasks
   task -l
   
   # Run specific steps
   task build
   task device
   task user
   # ... and so on
   
   # Clean the environment
   task clean
   ```

## Workflow Details

The workflow demonstrates:

- Device setup and vault creation
- Adding multiple devices to the same vault
- Splitting a secret across vault members
- Initiating and approving recovery requests
- Recovering the original secret

This provides a practical demonstration of the Shamir's Secret Sharing scheme implementation in the Meta Secret system. 