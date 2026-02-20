SHELL := /usr/bin/env bash

MODE ?= default
PROFILE ?= release
CHAIN ?= testnet
OUT_DIR ?= chain-specs/generated/$(MODE)

.PHONY: help \
	deploy-build deploy-specs deploy-verify deploy-smoke deploy-check \
	deploy-check-default deploy-check-production \
	deploy-specs-default deploy-specs-production \
	deploy-verify-default deploy-verify-production \
	run-node run-default-testnet run-production

help:
	@echo "Available targets:"
	@echo "  make deploy-build MODE=<default|production> PROFILE=<debug|release>"
	@echo "  make deploy-specs MODE=<default|production> OUT_DIR=<path>"
	@echo "  make deploy-verify MODE=<default|production> OUT_DIR=<path>"
	@echo "  make deploy-smoke MODE=<default|production> OUT_DIR=<path>"
	@echo "  make deploy-check MODE=<default|production>"
	@echo "  make run-node MODE=<default|production> CHAIN=<dev|testnet|production> PROFILE=<debug|release>"
	@echo ""
	@echo "Shortcuts:"
	@echo "  make deploy-check-default"
	@echo "  make deploy-check-production"
	@echo "  make deploy-specs-default"
	@echo "  make deploy-specs-production"
	@echo "  make deploy-verify-default"
	@echo "  make deploy-verify-production"
	@echo "  make run-default-testnet"
	@echo "  make run-production"

deploy-build:
	./scripts/deploy.sh build $(MODE) $(PROFILE)

deploy-specs:
	./scripts/deploy.sh specs $(MODE) $(OUT_DIR)

deploy-verify:
	./scripts/deploy.sh verify-specs $(OUT_DIR)

deploy-smoke:
	./scripts/deploy.sh smoke $(MODE) $(OUT_DIR)

deploy-check:
	./scripts/deploy.sh pipeline-check $(MODE)

deploy-check-default:
	$(MAKE) deploy-check MODE=default

deploy-check-production:
	$(MAKE) deploy-check MODE=production

deploy-specs-default:
	$(MAKE) deploy-specs MODE=default OUT_DIR=chain-specs/generated/default

deploy-specs-production:
	$(MAKE) deploy-specs MODE=production OUT_DIR=chain-specs/generated/production

deploy-verify-default:
	$(MAKE) deploy-verify MODE=default OUT_DIR=chain-specs/generated/default

deploy-verify-production:
	$(MAKE) deploy-verify MODE=production OUT_DIR=chain-specs/generated/production

run-node:
	./scripts/run-node.sh $(MODE) $(CHAIN) $(PROFILE)

run-default-testnet:
	$(MAKE) run-node MODE=default CHAIN=testnet PROFILE=release

run-production:
	$(MAKE) run-node MODE=production CHAIN=production PROFILE=release
