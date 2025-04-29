VAULT_NAME := $(shell head -c 8 /dev/urandom | xxd -p)

RELEASE_DIR := ../../target/release

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
accept_all:
	 cd a && ./meta-cli auth accept-all-join-requests

### 6. Show device A info (it must be a vault with 3 members)
device_a_info:
	 cd a && ./meta-cli info

all: init build device user sign_up accept_all device_a_info

clean:
	rm -rf a/meta-secret.redb
	rm -rf b/meta-secret.redb
	rm -rf c/meta-secret.redb
