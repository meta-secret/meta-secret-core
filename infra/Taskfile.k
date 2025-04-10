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
}

