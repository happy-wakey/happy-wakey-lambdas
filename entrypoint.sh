#!/usr/bin/env sh

# Keep the process contract identical for Docker, OCI runtimes, and local runs.
# LAMBDA_SIDECAR_PROC is an executable path/name, not an eval-able command line.
set -u

if [ "$#" -eq 0 ]; then
  echo "entrypoint.sh requires a command" >&2
  exit 64
fi

printf "command is '"
printf "%s " "$@"
printf "\n"

sidecar_proc="${LAMBDA_SIDECAR_PROC:-any_such_sidecar_proc}"
if ! command -v "$sidecar_proc" >/dev/null 2>&1; then
  echo "warning: sidecar '$sidecar_proc' is unavailable; forwarding combined stdout/stderr" >&2
  exec "$@"
fi

# A FIFO lets the sidecar observe both streams without changing the command's
# exit status. POSIX sh has no pipefail, so a plain `command 2>&1 | sidecar`
# would incorrectly report the sidecar's status as the workload status.
sidecar_dir="$(mktemp -d "${TMPDIR:-/tmp}/lambda-sidecar.XXXXXX")"
sidecar_fifo="$sidecar_dir/output"

cleanup() {
  if [ -p "$sidecar_fifo" ]; then
    unlink "$sidecar_fifo" || true
  fi
  if [ -d "$sidecar_dir" ]; then
    rmdir "$sidecar_dir" || true
  fi
}

trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

mkfifo "$sidecar_fifo"
"$sidecar_proc" < "$sidecar_fifo" &
sidecar_pid=$!

if "$@" >"$sidecar_fifo" 2>&1; then
  command_status=0
else
  command_status=$?
fi

if wait "$sidecar_pid"; then
  sidecar_status=0
else
  sidecar_status=$?
fi

if [ "$command_status" -ne 0 ]; then
  exit "$command_status"
fi
exit "$sidecar_status"
