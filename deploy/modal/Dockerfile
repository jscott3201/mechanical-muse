# BOPTEST v0.9.0's web, worker, redis and minio in one x86 image for Modal (deploy/modal/mm_boptest.py).
# Built with a BOPTEST v0.9.0 checkout as the context; the steps follow BOPTEST's own worker and
# web Dockerfiles. BOPTEST is pre-existing (see the README's dependency table); nothing here
# changes it.
FROM --platform=linux/x86_64 ubuntu:20.04
ENV DEBIAN_FRONTEND=noninteractive
USER root
ENV HOME /home/boptest
WORKDIR /
RUN apt-get update -y && apt-get install -y \
    ca-certificates wget curl libglu1 libgfortran4 libjpeg8 libfreetype6 libxi6 libdbus-glib-1-2 \
    libfontconfig1 libsm6 gdebi-core git build-essential redis-server python3 python3-pip \
 && pip3 install requests requests_toolbelt
# Worker (as service/worker/Dockerfile)
ENV SPAWN_VERSION light-0.4.3-7048a72798
RUN wget -q https://spawn.s3.amazonaws.com/builds/Spawn-$SPAWN_VERSION-Linux.tar.gz \
 && tar -xzf Spawn-$SPAWN_VERSION-Linux.tar.gz && rm Spawn-$SPAWN_VERSION-Linux.tar.gz
ENV PATH ${PATH}:/Spawn-$SPAWN_VERSION-Linux/bin
COPY service/worker/requirements/production.txt /boptest/
RUN wget -q https://repo.anaconda.com/miniconda/Miniconda3-py311_24.7.1-0-Linux-x86_64.sh -O /miniconda.sh \
 && /bin/bash /miniconda.sh -b -p /miniconda && rm /miniconda.sh \
 && . /miniconda/bin/activate \
 && conda create --name pyfmi3 python=3.11 -y \
 && conda activate pyfmi3 \
 && conda install -c conda-forge pyfmi=2.14 -y \
 && pip install -U pip setuptools \
 && python -m pip install -r /boptest/production.txt
COPY service/worker /worker
COPY testcase.py /boptest/lib/
COPY data /boptest/lib/data
COPY forecast /boptest/lib/forecast
COPY kpis /boptest/lib/kpis
COPY version.txt /boptest/
RUN touch /boptest/__init__.py /boptest/lib/__init__.py && ln -sf /boptest/version.txt /version.txt
# Web (as service/web/Dockerfile)
RUN curl -sL https://deb.nodesource.com/setup_16.x | bash - && apt-get install -y nodejs
COPY service/web/server/package.json service/web/server/package-lock.json /root/server/
WORKDIR /root/server
RUN npm install
COPY service/web/server /root/server
RUN npm run build
WORKDIR /
# minio at the release BOPTEST names, built from source (its published binaries and images are
# gone); same commit as deploy/minio/Dockerfile.
ARG MINIO_COMMIT=f19c534b9f457773dcd043d977433e1a71525c3b
RUN wget -q https://go.dev/dl/go1.24.2.linux-amd64.tar.gz && tar -C /usr/local -xzf go1.24.2.linux-amd64.tar.gz \
 && rm go1.24.2.linux-amd64.tar.gz \
 && git init -q /tmp/minio && cd /tmp/minio \
 && git fetch -q --depth 1 https://github.com/minio/minio "$MINIO_COMMIT" && git checkout -q FETCH_HEAD \
 && CGO_ENABLED=0 /usr/local/go/bin/go build -trimpath -o /usr/local/bin/minio . \
 && cd / && rm -rf /tmp/minio /root/go /root/.cache/go-build /usr/local/go
# Provisioning tool and the test cases we use (the office, and the home for price response)
COPY service/provision /src/service/provision
COPY testcases/multizone_office_complex_air /src/testcases/multizone_office_complex_air
COPY testcases/bestest_hydronic_heat_pump /src/testcases/bestest_hydronic_heat_pump
