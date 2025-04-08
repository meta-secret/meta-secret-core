import .common

apiVersion = "apps/v1"
kind = "StatefulSet"

metadata = {
    name = common.projectName
}

spec = {
    replicas = 1
    serviceName = common.projectName
    selector = {
        matchLabels = {
            app = common.projectName
        }
    }
    template = {
        metadata = {
            labels = {
                app = common.projectName
            }
        }
        spec = {
            containers = [
                {
                    name = common.projectName
                    imagePullPolicy = "Always"
                    image = option("imageName")
                    command = ["/bin/sh", "-c", "./meta-server"]
                }
            ]
            volumes = [
                {
                    name = "config"
                    configMap = {
                        name = "${common.projectName}-config"
                    }
                }
            ]
        }
    }
}
