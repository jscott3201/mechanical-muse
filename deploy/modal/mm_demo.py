"""Mechanical Muse on Modal: the same image as the repo's Dockerfile (web app served by mm-server),
as one Modal web endpoint. With nothing else set it runs in recorded mode: it verifies every spec
and plays back the recorded Muse Spark session, recorded BOPTEST runs and recorded explanations,
each labelled in the UI. See docs/packaging.md.

    modal serve deploy/modal/mm_demo.py      # temporary URL while the command runs
    modal deploy deploy/modal/mm_demo.py     # stays up (scales to zero when idle)
    modal app stop mm-demo                   # stop a deployed app

Run from the repo root (the image is built from it). Optional, read from your shell when you
serve or deploy:

    MM_BOPTEST_URL   a BOPTEST for live runs, such as the tunnel URL `make boptest-modal` prints.
                     Stored in a Modal secret, not in the image. BOPTEST has no authentication,
                     so do not post that URL.
    MM_MODAL_SECRET  the name of a Modal secret holding MODEL_API_KEY, for live Muse Spark
                     (`modal secret create mm-model-key MODEL_API_KEY=...`). Never put the key in
                     this file, the image, or a URL.

The web endpoint is public by default; anyone holding its URL can use it, including live model
calls billed to your key when MM_MODAL_SECRET is set.
"""

import os
import subprocess

import modal

ROOT = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))
PORT = 8080

image = modal.Image.from_dockerfile(
    os.path.join(ROOT, "Dockerfile"),
    context_dir=ROOT,
    # Same exclusions as `docker build` (no .env, runs/, research/, ledger/, target/ ...).
    ignore=modal.FilePatternMatcher.from_file(os.path.join(ROOT, ".dockerignore")),
    add_python="3.12",
)

secrets = []
# Only when serving or deploying from your machine; inside the container these are unset and the
# secrets already attached to the function apply.
if os.environ.get("MM_BOPTEST_URL"):
    secrets.append(modal.Secret.from_dict({"MM_BOPTEST_URL": os.environ["MM_BOPTEST_URL"]}))
if os.environ.get("MM_MODAL_SECRET"):
    secrets.append(modal.Secret.from_name(os.environ["MM_MODAL_SECRET"]))

app = modal.App("mm-demo", image=image)


# One container: mm-server keeps runs and sessions in memory and runs one live run at a time,
# so every request (and every event stream) must reach the same process.
@app.function(cpu=2, memory=2048, secrets=secrets, max_containers=1, scaledown_window=900, timeout=24 * 3600)
@modal.concurrent(max_inputs=200)
@modal.web_server(PORT, startup_timeout=60)
def web():
    # The image's ENTRYPOINT does not run on Modal, so start it the same way here. Runs and
    # verified drafts go to /app/runs inside the container and are lost when it scales down.
    # The Dockerfile's ENV sets the same values; repeat them so the server does not depend on how
    # the platform carries image environment over.
    env = dict(os.environ)
    for key, value in {
        "MM_ADDR": f"0.0.0.0:{PORT}",
        "MM_WEB_DIR": "/app/web",
        "MM_SPECS_DIR": "/app/specs",
        "MM_RUNS_DIR": "/app/runs",
        "MM_RECORDINGS_DIR": "/app/fixtures/recordings",
        "MM_REPLAYS_DIR": "/app/fixtures/replays",
        "MM_EXPLANATIONS_DIR": "/app/fixtures/explanations",
    }.items():
        env.setdefault(key, value)
    subprocess.Popen(["/usr/local/bin/mm-entrypoint"], cwd="/app", env=env)
