"""BOPTEST v0.9.0 on Modal (named mm_boptest so it never shadows BOPTEST's own `boptest` package
inside the image): one x86 container running BOPTEST's web, worker, redis and minio,
reached through a Modal tunnel. Run it with `make boptest-modal` (see docs/boptest-on-modal.md).

The tunnel URL is printed once; point mm-server at it with MM_BOPTEST_URL. Anyone holding the URL
can reach this BOPTEST (it has no authentication), so do not post it. The app stops after
`--minutes`, or with `modal app stop mm-boptest`.
"""

import os
import subprocess
import time

import modal

HERE = os.path.dirname(os.path.abspath(__file__))
# Needed only where the image is built (your machine); the module is imported again inside the
# container, where the image already exists.
BOPTEST_SRC = os.environ.get("BOPTEST_SRC", "/unused")

image = modal.Image.from_dockerfile(
    os.path.join(HERE, "Dockerfile"), context_dir=BOPTEST_SRC, add_python="3.11"
).add_local_file(os.path.join(HERE, "start.sh"), "/start.sh", copy=True)

app = modal.App("mm-boptest", image=image)


@app.function(cpu=4, memory=16384, timeout=4 * 3600)
def serve(minutes: int = 120):
    proc = subprocess.Popen(["bash", "/start.sh"])
    with modal.forward(80) as tunnel:
        print("BOPTEST_URL", tunnel.url, flush=True)
        deadline = time.time() + minutes * 60
        while time.time() < deadline and proc.poll() is None:
            time.sleep(10)
    proc.terminate()


@app.local_entrypoint()
def main(minutes: int = 120):
    if not os.path.isdir(os.path.join(BOPTEST_SRC, "testcases")):
        raise SystemExit("set BOPTEST_SRC (or BOPTEST_DIR for make) to a BOPTEST v0.9.0 checkout")
    serve.remote(minutes)
