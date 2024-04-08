include infra/artefacts/thalassa/common.mk

init: init_dirs

init_dirs:
	mkdir -p $(ARTEFACT_BUILD_DIR)

.PHONY: init init_dirs