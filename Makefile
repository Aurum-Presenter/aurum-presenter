.DEFAULT_GOAL := help
COMPOSE := docker compose
EXEC := $(COMPOSE) exec -T api aurum-api

.PHONY: help
help: ## Show this help
	@grep -hE '^[a-zA-Z_-]+:.*?## ' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-18s\033[0m %s\n", $$1, $$2}'

.env: ## Create .env from the template, generating both keys
	@test -f .env || (sed \
		-e "s|^APP_SIGNING_KEY=.*|APP_SIGNING_KEY=$$(openssl rand -hex 32)|" \
		-e "s|^APP_SECRET_KEY=.*|APP_SECRET_KEY=$$(openssl rand -base64 32)|" \
		.env-dist > .env && echo "created .env with fresh keys")

.PHONY: up
up: .env ## Start the backend stack
	$(COMPOSE) up -d --build
	$(EXEC) migrate

.PHONY: down
down: ## Stop the stack
	$(COMPOSE) down

.PHONY: logs
logs: ## Follow API logs
	$(COMPOSE) logs -f api

.PHONY: shell
shell: ## Shell into the API container
	$(COMPOSE) exec api sh

.PHONY: migrate
migrate: ## Apply migrations to control.sqlite and every workspace file
	$(EXEC) migrate

.PHONY: purge
purge: ## Purge applied ops, expired sessions, tombstones and old conflicts
	$(EXEC) maintenance:purge

.PHONY: workspaces
workspaces: ## List workspace database files
	$(EXEC) workspace:list

.PHONY: test
test: ## Run the Rust test suite: the rules, the API, and the files themselves
	cargo test --workspace

.PHONY: web-browser-test
web-browser-test: ## Run the client's browser tests (needs wasm-bindgen-cli and a matching chromedriver)
	cargo test -p aurum-web --target wasm32-unknown-unknown

.PHONY: lint
lint: ## Static analysis and formatting
	cargo clippy --workspace --all-targets -- -D warnings
	cargo fmt --check

.PHONY: mail
mail: ## Deliver queued email (invitations, password resets)
	$(EXEC) mail:send

.PHONY: signal
signal: ## Follow the stage-pairing relay, which shares the API's log
	$(COMPOSE) logs -f api

.PHONY: web-assets
web-assets: ## Install the client's non-Rust assets (Tailwind, Workbox, the Pdfium engine)
	cd web && npm install

.PHONY: web
web: web-assets ## Run the client's dev server, proxying the API
	trunk serve

.PHONY: web-build
web-build: web-assets ## Build the client into crates/web/dist
	trunk build --release

.PHONY: e2e
e2e: ## Drive the running stack through a browser (SPEC=sheets runs a subset)
	cd e2e && npm install --silent && node run.mjs $(SPEC)
