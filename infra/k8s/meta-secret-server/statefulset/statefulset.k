import ..common

_apple_service = option("appleEmailServiceId", type="str", default="")
_apple_callback = option("appleEmailCallbackUrl", type="str", default="")
_apple_origins = option("appleEmailWebOrigins", type="str", default="")
_apple_enabled = _apple_service != "" or _apple_callback != "" or _apple_origins != ""
assert not _apple_enabled or (_apple_service != "" and _apple_callback != "" and _apple_origins != ""), "Partial Apple configuration"

cfg = {
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
                        env = [
                            {name = "APPLE_EMAIL_SERVICE_ID", value = _apple_service}
                            {name = "APPLE_EMAIL_CALLBACK_URL", value = _apple_callback}
                            {name = "APPLE_EMAIL_WEB_ORIGINS", value = _apple_origins}
                        ] if _apple_enabled else []
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
}
