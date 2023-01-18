
clean: clean_core
	rm -rf ./target

clean_core:
	rm -rf ../core/src/models/

generate_schema:
	docker run --rm \
		-v ${PWD}:/local \
		-u 1000:1000 \
		openapitools/openapi-generator-cli:v6.2.1 generate \
		-i /local/schema.yaml \
		-g rust \
		-o /local/target/rust-models \
		--package-name meta-secret-core-models \
		--skip-validate-spec

	cp -r ./target/rust-models/src/models ../core/src/
	#cp -r ./target/rust-models/docs ../core/src/models/

generate_schema_swift:
	docker run --rm \
		-v ${PWD}:/local \
		-u 1000:1000 \
		openapitools/openapi-generator-cli:v6.2.1 generate \
		-i /local/schema.yaml \
		-g swift5 \
		-o /local/target/swift-models \
		--package-name meta-secret-core-models \
		--skip-validate-spec

#--global-property models
generate_schema_type_script:
	docker run --rm \
		-v ${PWD}:/local \
		-u 1000:1000 \
		openapitools/openapi-generator-cli:v6.2.1 generate \
		-i /local/schema.yaml \
		-g typescript-jquery \
		-o /local/target/core-models-ts \
		--package-name meta-secret-core-models \
		--additional-properties=supportsES6=true,typescriptThreePlus=true \
		--skip-validate-spec
