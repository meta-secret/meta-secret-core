## Deployment Automation

### This is a deployment automation guide
Let's go through and deploy meta-secret server on kubernetes

#### Docker Image
Before anything else, we need to rebuild the docker image for the meta-secret server.
```bash
docker buildx bake --push meta-server-image
```

#### Cluster Deployment
- Create a new cluster:
```bash
docker buildx bake taskomatic && docker run --rm \
  -v /var/run/docker.sock:/var/run/docker.sock \
  -v $HOME/.kube:/root/.kube \
  -v $HOME/.config/k3d:/root/.config/k3d \
  --workdir /taskomatic \
  localhost/taskomatic:latest k3d:delete_cluster

docker buildx bake taskomatic && docker run --rm \
  -v /var/run/docker.sock:/var/run/docker.sock \
  -v $HOME/.kube:/root/.kube \
  -v $HOME/.config/k3d:/root/.config/k3d \
  --workdir /taskomatic \
  localhost/taskomatic:latest k3d:create_cluster
```

- Deploy meta-secret server:
```bash
docker run --rm \
  -v /var/run/docker.sock:/var/run/docker.sock \
  -v $HOME/.kube:/root/.kube \
  -v $HOME/.config/k3d:/root/.config/k3d \
  --workdir /taskomatic \
  localhost/taskomatic:latest k8s:deploy_meta_server
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
