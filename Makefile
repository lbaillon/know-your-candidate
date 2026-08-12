.PHONY: dev lint typecheck test migrate

DATABASE_URL ?= postgresql://kyc:kyc@localhost:5432/kyc

dev:
	docker compose up -d
	$(MAKE) migrate
	@trap 'kill 0' EXIT; \
	(cd worker && cargo run) & \
	(cd backend && uv run uvicorn kyc_api.main:app --reload --port 8000) & \
	wait

lint:
	cd backend && uv run ruff check . && uv run ruff format --check .
	cd worker && cargo fmt --check && cargo clippy --all-targets -- -D warnings

typecheck:
	cd backend && uv run ty check

test:
	cd backend && uv run pytest
	cd worker && SQLX_OFFLINE=true cargo test

migrate:
	sqlx migrate run --source db/migrations --database-url "$(DATABASE_URL)"
