import ..common

cfg = {
    apiVersion = "v1"
    kind = "Service"
    metadata = {
        name = "${common.projectName}-service"
    }
    spec = {
        selector = {
            app = common.projectName
        }
        ports = [
            {
                name = "${common.projectName}-service-port"
                $protocol = "TCP"
                port = 3000
                targetPort = 3000
            }
        ]
    }
}
