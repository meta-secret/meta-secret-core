version = "3"
_vars = {
    cluster_name = "meta-secret"
    config_file = "k3d-config-dev.yaml"
    registry_name = "meta-secret-registry"
    registry_port = 5000
    image_name = "meta-secret-server:latest"
}

tasks = {
    default = {
        cmds = [
            "task -l"
        ]
        silent = True
    }

    create_cluster = {
        desc = "Create a k3d cluster with the specified config"
        cmds = [
            "echo Creating k3d cluster " + _vars.cluster_name + " with config " + _vars.config_file,
            "k3d cluster create --config " + _vars.config_file
        ]
    }

    delete_cluster = {
        desc = "Delete the k3d cluster"
        cmds = [
            "echo Deleting k3d cluster " + _vars.cluster_name,
            "k3d cluster delete " + _vars.cluster_name
        ]
    }

    get_kubeconfig = {
        desc = "Get kubeconfig for the k3d cluster"
        cmds = [
            "echo Getting kubeconfig for the k3d cluster " + _vars.cluster_name,
            "k3d kubeconfig get " + _vars.cluster_name
        ]
    }
}
