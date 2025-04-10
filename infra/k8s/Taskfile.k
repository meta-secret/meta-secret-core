version = "3"
_vars = {
    cluster_name = "meta-secret-dev"
    config_file = "k3d-config-dev.yaml"
    registry_name = "cypherkitty"
    image_name = "meta-secret-server:latest"
}

tasks = {
    default = {
        cmds = [
            "task -l"
        ]
        silent = True
    }

    deploy_meta_server = {
        cmds = [
            {task = "generate_meta_server_deployment_yaml"}
            "kubectl apply -f meta-secret-server/build/meta-server.yaml"
        ]
    }

    generate_meta_server_deployment_yaml = {
        _build_dir = "meta-secret-server/build"
        desc = "Deploy meta-secret server"
        cmds = [
            "mkdir -p " + _build_dir,
            "kcl meta-secret-server/main.k -D imageName=" + _vars.registry_name + "/" + _vars.image_name +" > " + _build_dir + "/meta-server.yaml"
        ]
    }
}
