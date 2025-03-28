from invoke import task, Context

CLUSTER_NAME = "meta-secret-dev"
CONFIG_FILE = "k3d-config-dev.yaml"
REGISTRY_NAME = "meta-secret-registry"
REGISTRY_PORT = 5000

@task
def create_cluster(c: Context):
    print(f"Creating k3d cluster '{CLUSTER_NAME}' with config '{CONFIG_FILE}'")
    c.run(f"k3d cluster create --config {CONFIG_FILE}")


@task
def delete_cluster(c: Context):
    print(f"Deleting k3d cluster '{CLUSTER_NAME}'")
    c.run(f"k3d cluster delete {CLUSTER_NAME}")


@task
def get_kubeconfig(c: Context):
    print(f"Getting kubeconfig for the k3d cluster '{CLUSTER_NAME}'")
    c.run(f"k3d kubeconfig get {CLUSTER_NAME}")
