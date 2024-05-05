INF_TARGETS_DIR=$(INF_BASE_DIR)/targets
INF_TARGETS_SPEC_DIR=$(INF_DOC_DIR)/targets

$(INF_TARGETS_DIR)/%.json: $(INF_TARGETS_SPEC_DIR)/%.md
	$(INF_STITCH_FROM_MD) $< json > $@

inf-targets-clean:
	rm $(INF_TARGETS_SPEC_DIR)/*.json