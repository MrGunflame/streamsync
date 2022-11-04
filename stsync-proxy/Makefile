SHELL := /bin/bash

.PHONY: docker

docker:
	cd .. && docker build --rm -t stsync-proxy -f stsync-proxy/Dockerfile .
