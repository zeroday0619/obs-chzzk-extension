FROM debian:trixie-slim

ENV DEBIAN_FRONTEND=noninteractive
ENV CARGO_HOME=/opt/cargo
ENV RUSTUP_HOME=/opt/rustup
ENV PATH=/opt/cargo/bin:${PATH}
ENV RUST_VERSION=1.94.1

RUN apt-get update \
 && apt-get install -y --no-install-recommends \
    build-essential \
    ca-certificates \
    cargo \
    curl \
    debhelper \
    devscripts \
    dpkg-dev \
    fakeroot \
    g++ \
    git \
    make \
    pkgconf \
    qt6-base-dev \
    rustc \
 && rm -rf /var/lib/apt/lists/*

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --profile minimal --default-toolchain "${RUST_VERSION}" \
 && rustc --version \
 && cargo --version

WORKDIR /work

COPY docker/debian-package/entrypoint.sh /usr/local/bin/debian-package

RUN chmod +x /usr/local/bin/debian-package

ENTRYPOINT ["debian-package"]
CMD ["scripts/build-deb.sh", "--clean"]
