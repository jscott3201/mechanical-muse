#!/bin/bash
# Starts BOPTEST's services on localhost inside the Modal container, then uploads the test case.
# The settings are BOPTEST's own local development values (its .env), with hosts set to localhost.
export NODE_ENV=production BOPTEST_TIMEOUT=900 BOPTEST_LOGLEVEL=INFO
export BOPTEST_REDIS_HOST=localhost
export BOPTEST_JOB_QUEUE_URL=http://goaws:4100/queue/local-queue1
export AWS_ACCESS_KEY_ID=xxxxxxxxxxxxxxxxxxxx AWS_SECRET_ACCESS_KEY=xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
export MINIO_ROOT_USER=user MINIO_ROOT_PASSWORD=password
export BOPTEST_INTERNAL_S3_URL=http://localhost:9000 BOPTEST_PUBLIC_S3_URL=http://localhost:9000
export BOPTEST_S3_BUCKET=boptest BOPTEST_REGION=us-west-1
export BOPTEST_DASHBOARD_SERVER=https://dashboard.boptest.net/
export BOPTEST_USE_TEST_USERS=true BOPTEST_TEST_USERNAME=canary BOPTEST_TEST_KEY=canarykey
export BOPTEST_TEST_PRIVILEGED_USERNAME=crusader BOPTEST_TEST_PRIVILEGED_KEY=crusaderkey
export BOPTEST_MESSAGE_TIMEOUT=3600000
export PYTHONPATH=/worker/jobs/:/boptest:/boptest/lib:/boptest/lib/kpis:/boptest/lib/forecast:/boptest/lib/data
redis-server --daemonize yes
mkdir -p /data
# Root credentials are the keys BOPTEST's services use, so no service account is needed.
(MINIO_ROOT_USER=$AWS_ACCESS_KEY_ID MINIO_ROOT_PASSWORD=$AWS_SECRET_ACCESS_KEY minio server /data --address :9000 > /tmp/minio.log 2>&1 &)
for i in $(seq 1 60); do curl -sf http://localhost:9000/minio/health/live && break; sleep 1; done
. /miniconda/bin/activate && conda activate pyfmi3
python - <<'PY'
import json, os, boto3
s3 = boto3.client("s3", endpoint_url="http://localhost:9000", region_name="us-west-1",
                  aws_access_key_id=os.environ["AWS_ACCESS_KEY_ID"],
                  aws_secret_access_key=os.environ["AWS_SECRET_ACCESS_KEY"])
s3.create_bucket(Bucket="boptest")
policy = {"Version": "2012-10-17", "Statement": [{"Effect": "Allow", "Principal": {"AWS": ["*"]},
          "Action": ["s3:GetObject"], "Resource": ["arn:aws:s3:::boptest/*"]}]}
s3.put_bucket_policy(Bucket="boptest", Policy=json.dumps(policy))
print("bucket ready")
PY
conda deactivate; conda deactivate
(cd / && . /miniconda/bin/activate && conda activate pyfmi3 && python -m worker > /tmp/worker.log 2>&1 &)
(cd /root/server && npm start > /tmp/web.log 2>&1 &)
for i in $(seq 1 120); do curl -sf http://localhost:80/version && break; sleep 1; done
echo
cd /src && BOPTEST_SERVER=http://localhost BOPTEST_DASHBOARD_API_KEY=crusaderkey PYTHONPATH=/src/service/provision \
  /usr/bin/python3 -m boptest_submit --shared --path ./testcases/
echo "PROVISIONED"
tail -f /tmp/web.log /tmp/worker.log
