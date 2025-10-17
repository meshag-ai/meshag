# Makefile for Meshag Infrastructure Services
# Run NATS and Valkey separately for development

ifeq (,$(wildcard ./.env))
    $(warning .env file not found. Environment variables might be missing.)
else
    include ./.env
    export
endif

.PHONY: nats valkey infra up load-env transport stt llm tts all build-service clean help

up:
	cd docker && docker compose down
	cd docker && docker compose up --build

load-env:
	@if [ -f .env ]; then \
		echo "Loading environment variables from .env file..."; \
		export $$(grep -v '^#' .env | grep -v '^$$' | xargs); \
		echo "Environment variables loaded successfully"; \
	else \
		echo "Error: .env file not found"; \
		exit 1; \
	fi

# Start NATS JetStream server
nats:
	@echo "Starting NATS JetStream server..."
	docker run -d \
		--name meshag-nats \
		--restart unless-stopped \
		-p 4222:4222 \
		-p 8222:8222 \
		-p 6222:6222 \
		-v nats_data:/data \
		nats:2.10-alpine \
		--jetstream \
		--store_dir=/data \
		--http_port=8222
	@echo "NATS server started on ports 4222 (client), 8222 (monitoring), 6222 (routing)"
	@echo "Monitor at: http://localhost:8222"

nats-ui:
	@echo "Starting NATS UI server..."
	docker run -d \
  		--name meshag-nats-ui \
  		--restart unless-stopped \
  		-p 31311:31311 \
  		-v nats_data:/db \
  		-e NATS_URL=nats://host.docker.internal:4222 \
  		ghcr.io/nats-nui/nui

# Start Valkey server
valkey:
	@echo "Starting Valkey server..."
	docker run -d \
		--name meshag-valkey \
		--restart unless-stopped \
		-p 6379:6379 \
		-v valkey_data:/data \
		valkey/valkey:7.2-alpine \
		valkey-server --appendonly yes
	@echo "Valkey server started on port 6379"

# Start both infrastructure services
infra: nats valkey
	@echo "Infrastructure services started!"
	@echo "NATS: nats://localhost:4222"
	@echo "Valkey: redis://localhost:6379"
	@echo "NATS Monitor: http://localhost:8222"


transport: load-env
	@echo "Starting transport service..."
	SERVICE_TYPE=transport cargo run -p meshag-service

t: transport

stt: load-env
	@echo "Starting STT service..."
	SERVICE_TYPE=stt cargo run -p meshag-service

s: stt

llm: load-env
	@echo "Starting LLM service..."
	SERVICE_TYPE=llm cargo run -p meshag-service

l: llm

tts: load-env
	@echo "Starting TTS service..."
	SERVICE_TYPE=tts cargo run -p meshag-service

tt: tts

all: load-env
	@echo "Starting all services..."
	@echo "Note: Use 'make up' to start all services with Docker Compose"
	@echo "Or run individual services with: make transport, make stt, make llm, make tts"

# Build the unified service
build-service:
	@echo "Building unified meshag-service..."
	cargo build -p meshag-service

# Clean build artifacts
clean:
	@echo "Cleaning build artifacts..."
	cargo clean

# Show help
help:
	@echo "Meshag Service Commands:"
	@echo "  make up          - Start all services with Docker Compose"
	@echo "  make infra       - Start NATS and Valkey infrastructure"
	@echo "  make nats        - Start NATS JetStream server"
	@echo "  make valkey      - Start Valkey server"
	@echo "  make transport   - Start transport service (development)"
	@echo "  make stt         - Start STT service (development)"
	@echo "  make llm         - Start LLM service (development)"
	@echo "  make tts         - Start TTS service (development)"
	@echo "  make build-service - Build unified service binary"
	@echo "  make clean       - Clean build artifacts"
	@echo "  make load-env    - Load environment variables from .env"
	@echo ""
	@echo "Shortcuts:"
	@echo "  make t           - Alias for transport"
	@echo "  make s           - Alias for stt"
	@echo "  make l           - Alias for llm"
	@echo "  make tt          - Alias for tts"
