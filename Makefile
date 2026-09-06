.DEFAULT_GOAL := help
COMPOSE := docker compose
EXEC := $(COMPOSE) exec -T api

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
	$(EXEC) php bin/aurum migrate

.PHONY: down
down: ## Stop the stack
	$(COMPOSE) down

.PHONY: logs
logs: ## Follow API logs
	$(COMPOSE) logs -f api

.PHONY: shell
shell: ## Shell into the API container
	$(COMPOSE) exec api bash

.PHONY: migrate
migrate: ## Apply migrations to control.sqlite and every workspace file
	$(EXEC) php bin/aurum migrate

.PHONY: purge
purge: ## Purge applied ops, expired sessions, tombstones and old conflicts
	$(EXEC) php bin/aurum maintenance:purge

.PHONY: workspaces
workspaces: ## List workspace database files
	$(EXEC) php bin/aurum workspace:list

.PHONY: test
test: ## Run the backend test suite
	$(EXEC) vendor/bin/phpunit

.PHONY: smoke
smoke: ## End-to-end check against the running stack
	API=http://localhost:$${API_PORT:-8080}/api/v1 \
		EXEC="$(COMPOSE) exec -T api" \
		DATA_DIR=/app/var/data \
		./scripts/smoke.sh

.PHONY: stan
stan: ## Static analysis
	$(EXEC) vendor/bin/phpstan analyse

.PHONY: mail
mail: ## Deliver queued email (invitations, password resets)
	$(EXEC) php bin/aurum mail:send

.PHONY: signal
signal: ## Follow the stage-pairing signalling relay's log
	$(COMPOSE) logs -f signal

.PHONY: web
web: ## Run the PWA dev server
	cd frontend && npm run dev

.PHONY: web-test
web-test: ## Run the PWA test suite (chart parsing, transposition, conversion)
	cd frontend && npm test
