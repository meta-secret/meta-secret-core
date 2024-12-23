
DOCKER_IMAGE:="ghcr.io/meta-secret/cli:latest"

docker_build:
	docker buildx build --tag ${DOCKER_IMAGE} --output type=docker .

docker_push: docker_build
	docker push ${DOCKER_IMAGE}

split: docker_build
	docker run -ti --rm -v secrets:/meta-secret/secrets ${DOCKER_IMAGE} split --secret $(secret)

docker_run_shell:
	docker run -ti --rm ${DOCKER_IMAGE} bash