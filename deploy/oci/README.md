# OCI deployment

This directory is the provider-neutral image hand-off point.

Build with Docker BuildKit:

~~~sh
docker buildx build --platform linux/amd64,linux/arm64 --file Dockerfile --tag happy-wakey-lambdas:dev .
~~~

Export a multi-platform OCI archive for an air-gapped or registry promotion
step:

~~~sh
mkdir -p tmp
docker buildx build --platform linux/amd64,linux/arm64 \
  --file Containerfile --output type=oci,dest=tmp/happy-wakey-lambdas.oci .
~~~

The runtime entrypoint is '/entrypoint.sh'. It invokes the command supplied by
the image and sends combined stdout/stderr through ' LAMBDA_SIDECAR_PROC '
when the internal sidecar is available. Set ' VCS_REF ' to the full source
commit for the OCI revision label, and promote by digest.

The same image can be used by Cloud Run, Azure custom/container handlers,
Scintilla, Kubernetes, and other OCI runtimes. AWS ZIP deployment remains a
separate ' provided.al2023 ' artifact with one ' bootstrap ' executable per
architecture.
