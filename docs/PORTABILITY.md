# Cross-platform build and invocation contract

This repository exposes one provider-neutral function contract. The product
handler remains the source of business behavior; provider adapters only decode
the platform event and map it to the shared request/response boundary.

## Docker and OCI images

' Dockerfile ' and ' Containerfile ' are equivalent multi-stage builds. The
builder selects a Rust binary and compiles it for the requested target; the
runtime stage contains only that binary and ' entrypoint.sh '. The default
' oci-http ' binary is a dependency-free health/invoke shim so a new repository
has a working image before product functions land.

Build a Docker multi-platform image:

~~~sh
docker buildx build \
  --platform linux/amd64,linux/arm64 \
  --file Dockerfile \
  --tag happy-wakey-lambdas:dev .
~~~

Export an OCI image layout without pushing it:

~~~sh
mkdir -p tmp
docker buildx build \
  --platform linux/amd64,linux/arm64 \
  --file Containerfile \
  --output type=oci,dest=tmp/happy-wakey-lambdas.oci .
~~~

Podman/Buildah can consume the same ' Containerfile ' and explicitly select
the OCI media type:

~~~sh
podman build --format=oci --arch amd64 --file Containerfile --tag happy-wakey-lambdas:amd64 .
podman build --format=oci --arch arm64 --file Containerfile --tag happy-wakey-lambdas:arm64 .
~~~

The build accepts ' BINARY ' and ' CARGO_FEATURES ' so an existing richer
adapter can use the same image contract:

~~~sh
docker buildx build --platform linux/amd64,linux/arm64 \
  --build-arg BINARY=worker-http --build-arg CARGO_FEATURES=http .
~~~

Use the full source commit as ' VCS_REF ' when publishing. Consumers should
deploy the digest, not a mutable branch tag.

## Sidecar-aware entrypoint

' entrypoint.sh ' prints the command as:

~~~text
command is '...'
~~~

Set ' LAMBDA_SIDECAR_PROC ' to the sidecar executable name or absolute path.
When it is available, the script sends both the workload's stdout and stderr
through one FIFO to the sidecar and preserves the workload exit status. If a
separate stderr sidecar is needed, put the fan-out/multiplexing logic in the
configured sidecar process. When the internal placeholder
' any_such_sidecar_proc ' is not present (for example during a local image
smoke test), the script warns and executes the workload directly; it never
treats a missing telemetry sidecar as a successful workload.

Sidecars must receive only bounded, sanitized logs. Credentials, bearer tokens,
private payloads, URLs containing secrets, and decrypted environment files
remain outside the image and the log stream.

## Provider mapping

| Platform | Adapter boundary | Artifact |
| --- | --- | --- |
| AWS Lambda | ' provided.al2023 '; use the Rust Lambda binary as ' bootstrap ' and build both ' arm64 ' and ' x86_64 ' ZIP variants | ZIP |
| Google Cloud Run/functions | ' PORT ' plus ' GET /healthz ', ' GET /readyz ', and ' POST /invoke ' | Docker or OCI image |
| Azure Functions | custom-handler HTTP forwarding to the same listener, or an Azure container plan | Docker or OCI image |
| Vercel | use the provider's function adapter with the shared envelope; the optional Node bundle is in ' node/ ' | single JS file or OCI image |
| Kubernetes/OCI hosts | run ' entrypoint.sh ' with the image command and platform-provided ' PORT ' | OCI image |
| Local | invoke the same HTTP routes or the portable provider adapter | source/binary |

Keep event decoding at the edge. The function core must not inspect AWS,
Google, Azure, or Vercel SDK objects and must not initialize migrations, ORM
pools, or unrelated listeners during cold start.

## Node, Deno, and Bun

' node/handler.mjs ' is a dependency-free provider-neutral handler. For Node
procedures, install the only build tool in the ' node/ ' package and emit one
bundle:

~~~sh
cd node
npm install
npm run bundle
~~~

The resulting ' dist/handler.bundle.mjs ' contains application code in one
file with no package-relative imports. Deno and Bun can produce native
single-file executables from the same source:

~~~sh
npm run bundle:deno
npm run bundle:bun
~~~

Platform-specific request/response shims should call the exported ' handler '
or ' invoke ' function; they must not duplicate business rules.

## Shared Scintilla runtime

' scintilla-run/scintilla-lambdas ' is the canonical shared lambda runtime and
provider/OCI contract package. Consumers declare it in the root ' .zpkg.toml '
under '[dependencies]', then resolve with ' zed install --frozen '. The
published package must be an immutable reviewed release; the generated
' .zpkg.lock ' is the source of the exact revision. The Scintilla package itself
is the publisher and therefore does not declare a dependency on itself.
