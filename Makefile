.PHONY: build up down logs test fmt

build:
	docker compose build

up:
	docker compose up -d --build

down:
	docker compose down

logs:
	docker compose logs -f openrouter-manager

test:
	cargo test
	cd frontend && npm install && npm run build

fmt:
	cargo fmt --all
