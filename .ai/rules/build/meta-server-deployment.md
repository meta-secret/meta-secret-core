# Meta-server deployment (Taskfile)

All commands from **repository root**. See [`.ai/skills/build-via-task/SKILL.md`](../../skills/build-via-task/SKILL.md).

## Build and push meta-server image

```bash
task meta-server   # local load
task push          # build + push default images (includes meta-server)
```

For meta-server image only via bake target `meta-server-image`, add a dedicated task if needed — do not run `docker buildx bake` directly.

## Taskomatic (k3d/k8s)

```bash
task taskomatic-run
```

Or manually after `task taskomatic`:

```bash
docker run --rm \
  -v /var/run/docker.sock:/var/run/docker.sock \
  -v $HOME/.kube:/root/.kube \
  -v $HOME/.config/k3d:/root/.config/k3d \
  --name taskomatic \
  --workdir /taskomatic \
  localhost/taskomatic:latest
```
