# This Makefile demonstrates the complete Meta Secret workflow in sequence.
# The numbered comments (1-10) indicate the order of operations in the secret
# sharing and recovery process. Run 'make run_all' to execute the entire workflow.

.PHONY: init build device user sign_up accept_all_join_requests device_a_info split send_recovery_request accept_recovery_request show_secret run_all clean

RELEASE_DIR := ../../target/release

# Configuration variables
VAULT_NAME := $(shell head -c 8 /dev/urandom | xxd -p)
PASS_NAME := test_pass
PASSWORD := p_ass_word

META_CLI_A := a/meta-cli
META_CLI_B := b/meta-cli
META_CLI_C := c/meta-cli

# Create working directories
init:
	mkdir -p a b c

# 1. Build meta-cli applications
build: init
	cargo build --release
	cp $(RELEASE_DIR)/meta-cli $(META_CLI_A)
	cp $(RELEASE_DIR)/meta-cli $(META_CLI_B)
	cp $(RELEASE_DIR)/meta-cli $(META_CLI_C)

# 2. Initialize devices with unique names
device: build
	cd a && ./meta-cli init device --device-name=device_a
	cd b && ./meta-cli init device --device-name=device_b
	cd c && ./meta-cli init device --device-name=device_c

# 3. Initialize users with the same vault name
user: device
	cd a && ./meta-cli init user --vault-name=$(VAULT_NAME)
	cd b && ./meta-cli init user --vault-name=$(VAULT_NAME)
	cd c && ./meta-cli init user --vault-name=$(VAULT_NAME)

# 4. Sign up all users - first user creates a new vault, others send join requests
sign_up: user
	cd a && ./meta-cli auth sign-up
	cd b && ./meta-cli auth sign-up
	cd c && ./meta-cli auth sign-up

# 5. Device A accepts all join requests
accept_all_join_requests: sign_up
	cd a && ./meta-cli auth accept-all-join-requests

# 6. Display device A vault information (it must be a vault with 3 members)
device_a_info: accept_all_join_requests
	cd a && ./meta-cli info

# 7. Split secret on device A and sync with others
split: device_a_info
	cd a && echo $(PASSWORD) | ./meta-cli secret split --pass-name=$(PASS_NAME) --stdin
	cd b && ./meta-cli info # download secret share
	cd c && ./meta-cli info # download secret share

# 8. Device A sends recovery request
send_recovery_request: split
	cd a && ./meta-cli secret recovery-request --pass-name=$(PASS_NAME)

# 9. Other devices accept recovery request
accept_recovery_request: send_recovery_request
	cd b && ./meta-cli secret accept-all-recovery-requests && ./meta-cli info
	cd c && ./meta-cli secret accept-all-recovery-requests && ./meta-cli info
	cd a && ./meta-cli info

# 10. Show recovered secret!
show_secret: accept_recovery_request
	@cd a && \
	CLAIM_ID=$$(./meta-cli info | grep "Claim #1" | awk -F'Id="' '{print $$2}' | awk -F'"' '{print $$1}') && \
	echo "Claim ID: $$CLAIM_ID" && \
	./meta-cli secret show --claim-id=$$CLAIM_ID

# Run entire demo sequence
run_all: clean show_secret

# Clean all generated files
clean:
	rm -rf a/meta-secret.redb b/meta-secret.redb c/meta-secret.redb
	rm -f $(META_CLI_A) $(META_CLI_B) $(META_CLI_C)
