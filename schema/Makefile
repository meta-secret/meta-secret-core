
clean: clean_core
	rm -rf ./target

clean_core:
	rm -rf ../core/src/models/

generate_schema:
	openapi-generator-cli generate \
		-i schema.yaml \
		-g rust \
		-o ./target/rust-models \
		--package-name meta-secret-core-models \
		--skip-validate-spec

	cp -r ./target/rust-models/src/models ../core/src/
	#cp -r ./target/rust-models/docs ../core/src/models/

generate_swift_schema:
	openapi-generator-cli generate \
		-i schema.yaml \
		-g swift5 \
		-o ./target/swift-models \
		--package-name meta-secret-core-models \
		--skip-validate-spec
