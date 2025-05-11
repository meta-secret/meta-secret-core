version = 3

includes = {
    k3d = {
        taskfile = "k3d/Taskfile.yml"
        dir = "k3d"
    }
    k8s = {
        taskfile = "k8s/Taskfile.yml"
        dir = "k8s"
    }
}

tasks = {
    default = {
        cmds = [
            "task -l"
        ]
        silent = True
    }

    generate_tasks = {
        desc = "Generate tasks"
        cmds = [
            "kcl Taskfile.k > Taskfile.yml"
            "cd k3d && kcl Taskfile.k > Taskfile.yml"
            "cd k8s && kcl Taskfile.k > Taskfile.yml"
        ]
    }

    redeploy_all = {
        desc = "Full redeploy"
        cmds = [
            {task = "k3d:delete_cluster"}
            {task = "k3d:create_cluster"}
            {task = "k8s:deploy_meta_server"}
        ]
    }
}

