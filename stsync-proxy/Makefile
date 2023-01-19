SHELL := /bin/sh

CARGO := cargo

FEATURES := log/release_max_level_info,tracing/release_max_level_info

.PHONY: build docker

all: build

build:
	$(CARGO) build --release --features=$(FEATURES)

docker:
	cd .. && docker build --rm -t stsync-proxy -f stsync-proxy/Dockerfile .
