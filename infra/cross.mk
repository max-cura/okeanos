cross-files := armv6zk-none-eabihf

$(okns-src-device)/%.json: $(okns-infra-target-dir)/%.md
	$(okns-infra-script-dir)/stitch-from-md.sh $< json > $@

.PHONY: clean-cross-targets all-cross-targets

all-cross-targets: $(okns-src-device)/$(cross-files:%=%.json)

clean-cross-targets:
	( cd $(okns-src-device) ; rm $(cross-files:%=%.json)