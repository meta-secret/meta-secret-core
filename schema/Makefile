
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

generate_schema_swift:
	openapi-generator-cli generate \
		-i schema.yaml \
		-g swift5 \
		-o ./target/swift-models \
		--package-name meta-secret-core-models \
		--skip-validate-spec

#--global-property models
generate_schema_type_script:
	openapi-generator-cli generate \
		-i schema.yaml \
		-g typescript-jquery \
		-o ./target/core-models-ts \
		--package-name meta-secret-core-models \
		--additional-properties=supportsES6=true,typescriptThreePlus=true \
		--skip-validate-spec
