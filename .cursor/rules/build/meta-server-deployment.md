## Deployment Automation

### This is a deployment automation guide
Let's go through and deploy meta-secret server on kubernetes

#### Docker Image
Before anything else, we need to rebuild the docker image for the meta-secret server.
```bash
cd meta-secret
earthly --allow-privileged --push +build-meta-server-image
```

#### Cluster Deployment
- Create a new cluster:
```bash
cd infra
earthly +taskomatic-run --task="k3d:delete_cluster"
earthly +taskomatic-run --task="k3d:create_cluster"
```

- Deploy meta-secret server:
```bash
earthly +taskomatic-run --task="k8s:deploy_meta_server"
sleep 1
```

- Verify the deployment:
```bash
kubectl get pods
```

- Verify meta-secret-server pod status:
```bash
kubectl describe pod meta-secret-server-0
```