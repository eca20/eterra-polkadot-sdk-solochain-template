SHELL := /usr/bin/env bash

MODE ?= default
PROFILE ?= release
CHAIN ?= testnet
ROLE ?=
OUT_DIR ?= chain-specs/generated/$(MODE)
SPEC ?= chain-specs/production-plain.json
PROD_CONFIG ?= chain-specs/production-overrides.json
PROD_OUT_DIR ?= chain-specs/finalized/$(MODE)
KEY_CONFIG ?= chain-specs/production-keys.json
NODE_BIN ?= ./target/debug/solochain-eterra-node

.PHONY: help \
	deploy-build deploy-specs deploy-verify deploy-verify-production deploy-generate-production-overrides deploy-finalize-production deploy-smoke deploy-check \
	deploy-check-default deploy-check-production \
	deploy-specs-default deploy-specs-production deploy-generate-production-overrides-production deploy-finalize-production-default deploy-finalize-production-production \
	deploy-verify-default deploy-verify-generated-production deploy-verify-production-path \
	run-node run-default-testnet run-production

help:
	@echo "Available targets:"
	@echo "  make deploy-build MODE=<default|production> PROFILE=<debug|release>"
	@echo "  make deploy-specs MODE=<default|production> OUT_DIR=<path>"
	@echo "  make deploy-verify MODE=<default|production> OUT_DIR=<path>"
	@echo "  make deploy-verify-production SPEC=<path/to/production-plain.json>"
	@echo "  make deploy-generate-production-overrides KEY_CONFIG=<path/to/keys.json> PROD_CONFIG=<path/to/overrides.json> NODE_BIN=<node-binary>"
	@echo "  make deploy-finalize-production MODE=<default|production> PROD_CONFIG=<path/to/config.json> PROD_OUT_DIR=<path>"
	@echo "  make deploy-smoke MODE=<default|production> OUT_DIR=<path>"
	@echo "  make deploy-check MODE=<default|production>"
	@echo "  make run-node MODE=<default|production> CHAIN=<dev|testnet|production> PROFILE=<debug|release> ROLE=<validator|full>"
	@echo ""
	@echo "Shortcuts:"
	@echo "  make deploy-check-default"
	@echo "  make deploy-check-production"
	@echo "  make deploy-specs-default"
	@echo "  make deploy-specs-production"
	@echo "  make deploy-generate-production-overrides-production"
	@echo "  make deploy-finalize-production-default"
	@echo "  make deploy-finalize-production-production"
	@echo "  make deploy-verify-default"
	@echo "  make deploy-verify-generated-production"
	@echo "  make deploy-verify-production"
	@echo "  make run-default-testnet"
	@echo "  make run-production"

deploy-build:
	./scripts/deploy.sh build $(MODE) $(PROFILE)

deploy-specs:
	./scripts/deploy.sh specs $(MODE) $(OUT_DIR)

deploy-verify:
	./scripts/deploy.sh verify-specs $(OUT_DIR)

deploy-verify-production:
	./scripts/deploy.sh verify-production $(SPEC)

deploy-generate-production-overrides:
	./scripts/generate-production-overrides.py --in $(KEY_CONFIG) --out $(PROD_CONFIG) --node-bin $(NODE_BIN)

deploy-finalize-production:
	./scripts/deploy.sh finalize-production-spec $(MODE) $(PROD_CONFIG) $(PROD_OUT_DIR)

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

deploy-generate-production-overrides-production:
	$(MAKE) deploy-generate-production-overrides KEY_CONFIG=chain-specs/production-keys.json PROD_CONFIG=chain-specs/production-overrides.json NODE_BIN=./target/debug/solochain-eterra-node

deploy-finalize-production-default:
	$(MAKE) deploy-finalize-production MODE=default

deploy-finalize-production-production:
	$(MAKE) deploy-finalize-production MODE=production

deploy-verify-default:
	$(MAKE) deploy-verify MODE=default OUT_DIR=chain-specs/generated/default

deploy-verify-generated-production:
	$(MAKE) deploy-verify MODE=production OUT_DIR=chain-specs/generated/production

deploy-verify-production-path:
	$(MAKE) deploy-verify-generated-production

run-node:
	./scripts/run-node.sh $(MODE) $(CHAIN) $(PROFILE) $(ROLE)

run-default-testnet:
	$(MAKE) run-node MODE=default CHAIN=testnet PROFILE=release

run-production:
	$(MAKE) run-node MODE=production CHAIN=production PROFILE=release
