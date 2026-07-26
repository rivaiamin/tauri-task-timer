.PHONY: build claude-add claude-remove

MCP_BIN := $(abspath dist/index.js)

build:
	pnpm build

# Register this MCP server in Claude Code (reads BASE_URL + API_KEY from .env).
claude-add: build
	@test -f .env || (echo "missing .env — copy .env.example" && exit 1)
	set -a && . ./.env && set +a && \
	test -n "$$BASE_URL" && test -n "$$API_KEY" && \
	claude mcp add task-timer \
		--env BASE_URL="$$BASE_URL" \
		--env API_KEY="$$API_KEY" \
		-- node "$(MCP_BIN)"

claude-remove:
	claude mcp remove task-timer
