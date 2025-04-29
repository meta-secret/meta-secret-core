RELEASE_DIR := ../../target/release

VAULT_NAME := $(shell head -c 8 /dev/urandom | xxd -p)
PASS_NAME := test_pass

init:
	mkdir -p a
	mkdir -p b
	mkdir -p c

### 1. Build three meta-cli apps
build:
	cargo build --release
	cp $(RELEASE_DIR)/meta-cli a/meta-cli
	cp $(RELEASE_DIR)/meta-cli b/meta-cli
	cp $(RELEASE_DIR)/meta-cli c/meta-cli

### 2. Initialize devices
device:
	cd a && ./meta-cli init device --device-name=device_a
	cd b && ./meta-cli init device --device-name=device_b
	cd c && ./meta-cli init device --device-name=device_c

### 3. Initialize users (define vault name)
user:
	cd a && ./meta-cli init user --vault-name=$(VAULT_NAME)
	cd b && ./meta-cli init user --vault-name=$(VAULT_NAME)
	cd c && ./meta-cli init user --vault-name=$(VAULT_NAME)

### 4. First user creates a new vault, others send join requests
sign_up:
	cd a && ./meta-cli auth sign-up
	cd b && ./meta-cli auth sign-up
	cd c && ./meta-cli auth sign-up

### 5. Device A will accept all join requests
accept_all_join_requests:
	cd a && ./meta-cli auth accept-all-join-requests

### 6. Show device A info (it must be a vault with 3 members)
device_a_info:
	cd a && ./meta-cli info

### 7. Device A: Split
split:
	cd a && ./meta-cli secret split --pass=p_ass_word --pass-name=$(PASS_NAME)
	cd b && ./meta-cli info #download secret share
	cd c && ./meta-cli info #download secret share

### 8. Device A: Send recovery request
send_recovery_request:
	cd a && ./meta-cli secret recovery-request --pass-name=$(PASS_NAME)

### 9. Accept recovery request by other devices
accept_recovery_request:
	cd b && ./meta-cli secret accept-all-recovery-requests && ./meta-cli info
	cd c && ./meta-cli secret accept-all-recovery-requests && ./meta-cli info
	cd a && ./meta-cli info

### 10. Show recovered secret!
show_secret:
	# extract recovery claim id, and the show the secret
	@cd a && \
    	CLAIM_ID=$$(./meta-cli info | grep "Claim #1" | awk -F'Id="' '{print $$2}' | awk -F'"' '{print $$1}'); \
    	echo "Claim ID: $$CLAIM_ID"; \
    	./meta-cli secret show --claim-id=$$CLAIM_ID

all: \
	init \
	build \
	device \
	user \
	sign_up \
	accept_all_join_requests \
	device_a_info \
	split \
	send_recovery_request \
	accept_recovery_request \
	show_secret

clean:
	rm -rf a/meta-secret.redb
	rm -rf b/meta-secret.redb
	rm -rf c/meta-secret.redb
