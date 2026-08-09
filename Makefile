.PHONY: build up down logs test fmt release-tag

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

release-tag:
	@if [ -z "$(VERSION)" ]; then echo "Usage: make release-tag VERSION=v3.0.0"; exit 1; fi
	git tag -a $(VERSION) -m "Release $(VERSION)"
	git push origin $(VERSION)

