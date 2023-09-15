
clean: clean_core
	rm -rf ./target

clean_core:
	rm -rf ../core/src/models/

generate_schema:
	docker run --rm \
		-v ${PWD}:/openapi openapitools/openapi-generator-cli:v7.0.0 generate \
		-i /openapi/schema.yaml \
		-g rust \
		-o /openapi/target/rust-models \
		--package-name meta-secret-core-models \
		--skip-validate-spec

	cp -r ./target/rust-models/src/models ../core/src/
	#cp -r ./target/rust-models/docs ../core/src/models/

generate_schema_swift:
	docker run --rm \
		-v ${PWD}:/openapi openapitools/openapi-generator-cli:v7.0.0 generate \
		-i /openapi/schema.yaml \
		-g swift5 \
		-o /openapi/target/swift-models \
		--package-name meta-secret-core-models \
		--skip-validate-spec

#--global-property models
generate_schema_type_script:
	docker run --rm \
		-v ${PWD}:/openapi openapitools/openapi-generator-cli:v7.0.0 generate \
		-i /openapi/schema.yaml \
		-g typescript-jquery \
		-o /openapi/target/core-models-ts \
		--package-name meta-secret-core-models \
		--additional-properties=supportsES6=true,typescriptThreePlus=true \
		--skip-validate-spec
