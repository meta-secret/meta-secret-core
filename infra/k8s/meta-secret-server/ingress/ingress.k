import ..common

cfg = {
    apiVersion = "networking.k8s.io/v1"
    kind = "Ingress"
    metadata = {
        name: "${common.projectName}-ingress"
        annotations = {
            "ingress.kubernetes.io/ssl-redirect" = "false"
        }
    }
    spec = {
        rules = [
            {
                #host = "meta-secret.metaelon.space"
                http = {
                    paths = [
                        {
                            path = "/"
                            pathType = "Prefix"
                            backend = {
                                service = {
                                    name = "${common.projectName}-service"
                                    port = {
                                        number = 3000
                                    }
                                }
                            }
                        }
                    ]
                }
            }
        ]
    }
}
