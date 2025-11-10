.PHONY: help build run-gateway run-llm run-stt run-tts test clean docker-build docker-up docker-down

# Default target
help:
	@echo "Meshag - Available commands:"
	@echo ""
	@echo "  make build          - Build release binary"
	@echo "  make test           - Run all tests"
	@echo "  make docker-build   - Build Docker image"
	@echo "  make docker-up      - Start all services with Docker Compose"
	@echo "  make docker-down    - Stop all Docker services"
	@echo "  make clean          - Clean build artifacts"
	@echo ""
	@echo "Run individual services (requires NATS running):"
	@echo "  make run-gateway    - Run API Gateway (port 8080)"
	@echo "  make run-llm        - Run LLM service"
	@echo "  make run-stt        - Run STT service"
	@echo "  make run-tts        - Run TTS service"

# Build
build:
	cargo build --release --bin meshag-service

# Run individual services
run-gateway:
	SERVICE_TYPE=gateway cargo run --release --bin meshag-service

run-llm:
	SERVICE_TYPE=llm cargo run --release --bin meshag-service

run-stt:
	SERVICE_TYPE=stt cargo run --release --bin meshag-service

run-tts:
	SERVICE_TYPE=tts cargo run --release --bin meshag-service

# Testing
test:
	cargo test --workspace

test-verbose:
	cargo test --workspace -- --nocapture

# Docker
docker-build:
	docker build -f docker/Dockerfile -t meshag-service:latest .

docker-up:
	@if [ ! -f .env ]; then echo "Error: .env file not found. Copy .env.example to .env and add your API keys"; exit 1; fi
	cd docker && docker-compose up -d

docker-down:
	cd docker && docker-compose down

docker-logs:
	cd docker && docker-compose logs -f

docker-rebuild:
	cd docker && docker-compose down
	cd docker && docker-compose build --no-cache
	cd docker && docker-compose up -d

# Development helpers
dev-nats:
	docker run -d --name nats -p 4222:4222 -p 8222:8222 nats:2.10-alpine -js -m 8222

dev-nats-stop:
	docker stop nats && docker rm nats

# Clean
clean:
	cargo clean
	cd docker && docker-compose down -v
	-docker rmi meshag-service:latest

# Check
check:
	cargo check --workspace
	cargo clippy --workspace -- -D warnings
	cargo fmt --check

# Format
fmt:
	cargo fmt --all

# Install dependencies for development
install-deps:
	rustup update
	cargo install cargo-watch
	cargo install cargo-edit
