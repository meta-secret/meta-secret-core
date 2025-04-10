import manifests
import .service
import .statefulset
import .ingress

_configs = [statefulset.cfg, service.cfg, ingress.cfg]
manifests.yaml_stream(_configs)