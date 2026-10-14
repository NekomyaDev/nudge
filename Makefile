# Nudge Standalone Agent Packaging Makefile
#
# Provides simple targets to package .ndg programs into standalone agent binaries.
#
# Targets:
#   agent:       Package a Nudge program into a full (live-capable) standalone agent.
#                Usage: make agent PROG=examples/simple_add.ndg [OUT=simple_add] [FN=main]
#
#   agent-small: Package a Nudge program into an offline replay-only standalone agent (~866KB).
#                Usage: make agent-small PROG=examples/hello_llm.ndg [OUT=hello_llm] [FN=main]
#
#   clean-agent: Remove cached replay runtime sidecar and temporary artifacts.
#
#   help:        Show this help message.

.PHONY: help agent agent-small clean-agent

SCRIPT := scripts/build_agent.sh

help:
	@echo "Nudge Standalone Agent Packaging"
	@echo ""
	@echo "Targets:"
	@echo "  make agent PROG=<prog.ndg> [OUT=<out>] [FN=<entry>]"
	@echo "      Build full standalone agent binary (live LLM capable, ~2.1MB)."
	@echo ""
	@echo "  make agent-small PROG=<prog.ndg> [OUT=<out>] [FN=<entry>]"
	@echo "      Build lightweight offline replay-only agent binary (~866KB)."
	@echo ""
	@echo "  make clean-agent"
	@echo "      Remove cached replay runtime sidecar and temporary artifacts."

agent:
	@if [ -z "$(PROG)" ]; then \
		echo "error: PROG is required. Usage: make agent PROG=path/to/prog.ndg [OUT=path/to/out] [FN=entry]" >&2; \
		exit 1; \
	fi
	@bash $(SCRIPT) "$(PROG)" $(if $(OUT),"$(OUT)",) $(if $(FN),--fn "$(FN)",)

agent-small:
	@if [ -z "$(PROG)" ]; then \
		echo "error: PROG is required. Usage: make agent-small PROG=path/to/prog.ndg [OUT=path/to/out] [FN=entry]" >&2; \
		exit 1; \
	fi
	@bash $(SCRIPT) "$(PROG)" $(if $(OUT),"$(OUT)",) $(if $(FN),--fn "$(FN)",) --replay-only

clean-agent:
	@echo "==> Cleaning agent build artifacts..."
	@rm -f target/release/nudge-agent-replay
	@echo "Done."
